//! The `/events` endpoint: the member's rooms as a backlog, `Ready`, the app release the server
//! announces (if it knows one), then live events until the stream or its connection ends.

use std::time::Duration;

use schwitzer_proto::{Event, MAX_RPC_BYTES, Subscribe, from_bytes};
use tokio::time::timeout;
use weida::{ErrorCode, IncomingRequest, OutgoingTransfer, Replier, TransferMeta};

use crate::State;
use crate::auth;
use crate::hub::{Frame, frame_of};
use crate::store;

/// How long one frame may take to be written before the stream is dropped.
const STALL: Duration = Duration::from_secs(30);

/// Accepts subscriptions until the replier closes, each in its own task.
pub async fn serve(state: State, replier: Replier) {
    while let Ok(request) = replier.accept().await {
        tokio::spawn(handle(state.clone(), request));
    }
}

async fn write(out: &mut OutgoingTransfer, frame: &Frame) -> Result<(), ()> {
    match timeout(STALL, out.write_all(frame)).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

async fn handle(state: State, mut request: IncomingRequest) {
    let Ok(member) = auth::caller(&request) else {
        request.refuse(ErrorCode::Rejected).await;
        return;
    };
    let body = timeout(STALL, request.body().read_capped(MAX_RPC_BYTES)).await;
    match body {
        Ok(Ok(body)) if from_bytes::<Subscribe>(&body).is_ok() => {}
        _ => {
            request.refuse(ErrorCode::Rejected).await;
            return;
        }
    }

    // Register before the backlog is read, so nothing published meanwhile is lost.
    let mut sub = state.hub.subscribe(member);
    if sub.came_online {
        let st = state.clone();
        let announced = state
            .store
            .run(move |conn| st.announce(conn, &member))
            .await;
        if let Err(e) = announced {
            tracing::warn!("announcing presence failed: {e}");
        }
    }
    let gone = request.canceled();
    let run = async {
        let Ok(mut out) = request.reply(TransferMeta::default()).await else {
            return;
        };
        let st = state.clone();
        let rooms = state
            .store
            .run(move |conn| {
                store::rooms_of(conn, &member)?
                    .iter()
                    .map(|room| st.room_state(conn, room))
                    .collect::<Result<Vec<_>, _>>()
            })
            .await;
        let rooms = match rooms {
            Ok(rooms) => rooms,
            Err(e) => {
                tracing::error!("event backlog failed: {e}");
                return;
            }
        };
        for room in rooms {
            if let Some(frame) = frame_of(&Event::Room(room))
                && write(&mut out, &frame).await.is_err()
            {
                return;
            }
        }
        if let Some(frame) = frame_of(&Event::Ready)
            && write(&mut out, &frame).await.is_err()
        {
            return;
        }
        // Read after subscribing; the release check stores before it publishes, so a change
        // meanwhile arrives at least once.
        let release = state
            .release
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(version) = release
            && let Some(frame) = frame_of(&Event::Release {
                version: version.to_string(),
            })
            && write(&mut out, &frame).await.is_err()
        {
            return;
        }
        tokio::pin!(gone);
        loop {
            let frame = tokio::select! {
                frame = sub.rx.recv() => frame,
                () = &mut gone => None,
            };
            let Some(frame) = frame else { return };
            if write(&mut out, &frame).await.is_err() {
                return;
            }
        }
    };
    run.await;

    if state.hub.unsubscribe(&member, sub.id) {
        let st = state.clone();
        let left = state
            .store
            .run(move |conn| {
                st.sessions.clear_member(&member);
                for room in store::rooms_of(conn, &member)? {
                    let ids = store::room_member_ids(conn, &room)?;
                    st.hub.publish(
                        &ids,
                        &Event::Sessions {
                            room,
                            member,
                            sessions: Vec::new(),
                        },
                    );
                }
                for (room, session) in st.debug.remove_member_everywhere(&member) {
                    let ids = store::room_member_ids(conn, &room)?;
                    st.hub.publish(&ids, &Event::Debug { room, session });
                }
                st.announce(conn, &member)
            })
            .await;
        if let Err(e) = left {
            tracing::warn!("announcing absence failed: {e}");
        }
    }
}
