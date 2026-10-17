//! The `/rpc` endpoint: one request in, one reply out.
//!
//! Every handler that changes state runs inside one store transaction and
//! publishes its events right after the commit, still under the store's lock:
//! events therefore reach subscribers in commit order.

use rusqlite::Connection;
use schwitzer_proto::{
    ApiError, Event, MAX_RPC_BYTES, Member, MemberId, PROTOCOL_VERSION, RPC_TIMEOUT, Reply,
    Request, Response, RoomId, RoomToken, SessionInfo, Task, caps, from_bytes, now_ms, to_bytes,
};
use tokio::time::timeout;
use weida::{ErrorCode, IncomingRequest, Replier, TransferMeta};

use crate::State;
use crate::auth;
use crate::store::{self, R};

/// Accepts requests until the replier closes, each in its own task.
pub async fn serve(state: State, replier: Replier) {
    while let Ok(request) = replier.accept().await {
        tokio::spawn(handle(state.clone(), request));
    }
}

async fn handle(state: State, mut request: IncomingRequest) {
    let caller = match auth::caller(&request) {
        Ok(caller) => caller,
        Err(e) => {
            respond(request, Err(e)).await;
            return;
        }
    };
    let body = timeout(RPC_TIMEOUT, request.body().read_capped(MAX_RPC_BYTES)).await;
    let reply: Reply = match body {
        Ok(Ok(body)) => match from_bytes::<Request>(&body) {
            Ok(req) => dispatch(&state, caller, req).await,
            Err(_) => Err(ApiError::Invalid("malformed request".into())),
        },
        Ok(Err(weida::Error::TooLarge { .. })) => {
            Err(ApiError::Invalid("request too large".into()))
        }
        Ok(Err(_)) | Err(_) => {
            request.refuse(ErrorCode::Rejected).await;
            return;
        }
    };
    respond(request, reply).await;
}

/// Sends `reply` as the answer to `request`.
async fn respond(request: IncomingRequest, reply: Reply) {
    let bytes = to_bytes(&reply);
    let bytes = if bytes.len() > MAX_RPC_BYTES {
        to_bytes::<Reply>(&Err(ApiError::Internal))
    } else {
        bytes
    };
    let sent = timeout(RPC_TIMEOUT, async {
        let mut out = request.reply(TransferMeta::default()).await?;
        out.write_all(&bytes).await?;
        out.finish()
    })
    .await;
    match sent {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::debug!("reply failed: {e}"),
        Err(_) => tracing::debug!("reply timed out"),
    }
}

/// A text field: trimmed, at most `cap` bytes, and non-empty if `required`.
fn text(field: &str, value: &str, cap: usize, required: bool) -> Result<String, ApiError> {
    let value = value.trim();
    if required && value.is_empty() {
        return Err(ApiError::Invalid(format!("{field} is empty")));
    }
    if value.len() > cap {
        return Err(ApiError::Invalid(format!(
            "{field} is too long (at most {cap} bytes)"
        )));
    }
    Ok(value.to_owned())
}

/// Display metadata: trimmed and cut to `cap` bytes at a character boundary.
fn clamp(value: &str, cap: usize) -> String {
    let value = value.trim();
    let mut end = value.len().min(cap);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

async fn dispatch(state: &State, caller: MemberId, req: Request) -> Reply {
    match req {
        Request::Hello { protocol, name } => {
            if protocol != PROTOCOL_VERSION {
                return Err(ApiError::Protocol);
            }
            let name = text("name", &name, caps::MEMBER_NAME, true)?;
            set_name(state, caller, name, false).await?;
            Ok(Response::Hello { member: caller })
        }
        Request::SetName { name } => {
            let name = text("name", &name, caps::MEMBER_NAME, true)?;
            set_name(state, caller, name, true).await?;
            Ok(Response::Ok)
        }
        Request::CreateRoom { name } => {
            let name = text("room name", &name, caps::ROOM_NAME, true)?;
            create_room(state, caller, name).await
        }
        Request::JoinRoom { token } => join_room(state, caller, token).await,
        Request::LeaveRoom { room } => leave_room(state, caller, room).await,
        Request::Propose { room, title, body } => {
            let title = text("title", &title, caps::TITLE, true)?;
            let body = text("body", &body, caps::BODY, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::propose(conn, &room, &caller, &title, &body, now).map(Some)
                })
                .await,
            )
        }
        Request::EditTask {
            room,
            task,
            title,
            body,
            plan,
        } => {
            let title = text("title", &title, caps::TITLE, true)?;
            let body = text("body", &body, caps::BODY, false)?;
            let plan = text("plan", &plan, caps::PLAN, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::edit_task(conn, &room, task, &title, &body, &plan, now).map(Some)
                })
                .await,
            )
        }
        Request::SetPriority {
            room,
            task,
            priority,
        } => task_reply(
            mutate(state, caller, room, move |conn, now| {
                store::set_priority(conn, &room, task, priority, now).map(Some)
            })
            .await,
        ),
        Request::Decide {
            room,
            task,
            decision,
        } => task_reply(
            mutate(state, caller, room, move |conn, now| {
                store::decide(conn, &room, task, decision, now).map(Some)
            })
            .await,
        ),
        Request::Claim {
            room,
            work,
            task,
            agent,
        } => {
            let agent = text("agent", &agent, caps::AGENT_LABEL, false)?;
            let claimed = mutate(state, caller, room, move |conn, now| {
                store::claim(conn, &room, &caller, work, task, &agent, now)
            })
            .await?;
            Ok(Response::Claimed(claimed))
        }
        Request::SubmitPlan { room, task, plan } => {
            let plan = text("plan", &plan, caps::PLAN, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::submit_plan(conn, &room, &caller, task, &plan, now).map(Some)
                })
                .await,
            )
        }
        Request::Finish { room, task, note } => {
            let note = text("note", &note, caps::NOTE, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::finish(conn, &room, &caller, task, &note, now).map(Some)
                })
                .await,
            )
        }
        Request::Release { room, task, note } => {
            let note = text("note", &note, caps::NOTE, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::release(conn, &room, &caller, task, &note, now).map(Some)
                })
                .await,
            )
        }
        Request::AddNote {
            room,
            task,
            text: note,
            agent,
        } => {
            let note = text("text", &note, caps::NOTE, true)?;
            let agent = match agent {
                Some(label) => Some(text("agent", &label, caps::AGENT_LABEL, false)?)
                    .filter(|label| !label.is_empty()),
                None => None,
            };
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::add_note(conn, &room, &caller, task, &note, agent.as_deref(), now)
                        .map(Some)
                })
                .await,
            )
        }
        Request::PublishSessions { room, sessions } => {
            publish_sessions(state, caller, room, sessions).await
        }
    }
}

fn task_reply(result: Result<Option<Task>, ApiError>) -> Reply {
    match result? {
        Some(task) => Ok(Response::Task(task)),
        None => Err(ApiError::Internal),
    }
}

/// Runs a task mutation in one transaction as a member of `room`, then
/// publishes the resulting task to the room's members.
async fn mutate<F>(
    state: &State,
    caller: MemberId,
    room: RoomId,
    f: F,
) -> Result<Option<Task>, ApiError>
where
    F: FnOnce(&Connection, u64) -> R<Option<Task>> + Send + 'static,
{
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::require_member(&tx, &room, &caller)?;
            let task = f(&tx, now_ms())?;
            let members = store::room_member_ids(&tx, &room)?;
            tx.commit()?;
            if let Some(task) = &task {
                st.hub.publish(
                    &members,
                    &Event::Task {
                        room,
                        task: task.clone(),
                    },
                );
            }
            Ok(task)
        })
        .await
}

/// Stores the caller's name and tells their rooms; `always` also when it did
/// not change.
async fn set_name(
    state: &State,
    caller: MemberId,
    name: String,
    always: bool,
) -> Result<(), ApiError> {
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let changed = store::upsert_person(conn, &caller, &name)?;
            if changed || always {
                st.announce(conn, &caller)?;
            }
            Ok(())
        })
        .await
}

async fn create_room(state: &State, caller: MemberId, name: String) -> Reply {
    let token = RoomToken::random();
    let id = RoomId::of(&token);
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::create_room(&tx, &id, &name, &caller, now_ms())?;
            let room = st.room_state(&tx, &id)?;
            tx.commit()?;
            st.hub.publish(&[caller], &Event::Room(room));
            Ok(())
        })
        .await?;
    Ok(Response::Room { id, token })
}

async fn join_room(state: &State, caller: MemberId, token: RoomToken) -> Reply {
    let id = RoomId::of(&token);
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            if !store::room_exists(&tx, &id)? {
                return Err(ApiError::NoSuchRoom.into());
            }
            let added = store::add_member(&tx, &id, &caller, now_ms())?;
            if !added {
                return Ok(());
            }
            let room = st.room_state(&tx, &id)?;
            let others: Vec<MemberId> = store::room_member_ids(&tx, &id)?
                .into_iter()
                .filter(|m| *m != caller)
                .collect();
            let member = Member {
                id: caller,
                name: store::person_name(&tx, &caller)?,
                online: st.hub.is_online(&caller),
            };
            tx.commit()?;
            st.hub.publish(&[caller], &Event::Room(room));
            st.hub.publish(&others, &Event::Member { room: id, member });
            Ok(())
        })
        .await?;
    Ok(Response::Room { id, token })
}

async fn leave_room(state: &State, caller: MemberId, room: RoomId) -> Reply {
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::require_member(&tx, &room, &caller)?;
            store::remove_member(&tx, &room, &caller)?;
            st.sessions.clear(&room, &caller);
            let others = store::room_member_ids(&tx, &room)?;
            let member = Member {
                id: caller,
                name: store::person_name(&tx, &caller)?,
                online: false,
            };
            tx.commit()?;
            st.hub.publish(&[caller], &Event::Left { room });
            st.hub.publish(&others, &Event::Member { room, member });
            st.hub.publish(
                &others,
                &Event::Sessions {
                    room,
                    member: caller,
                    sessions: Vec::new(),
                },
            );
            Ok(())
        })
        .await?;
    Ok(Response::Ok)
}

async fn publish_sessions(
    state: &State,
    caller: MemberId,
    room: RoomId,
    sessions: Vec<SessionInfo>,
) -> Reply {
    if sessions.len() > caps::SESSIONS_PER_PUBLISH {
        return Err(ApiError::Invalid(format!(
            "sessions has more than {} entries",
            caps::SESSIONS_PER_PUBLISH
        )));
    }
    let sessions: Vec<SessionInfo> = sessions
        .into_iter()
        .map(|s| SessionInfo {
            id: clamp(&s.id, caps::SESSION_TITLE),
            title: clamp(&s.title, caps::SESSION_TITLE),
            project: clamp(&s.project, caps::SESSION_TITLE),
            model: clamp(&s.model, caps::SESSION_TITLE),
            ..s
        })
        .collect();
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            store::require_member(conn, &room, &caller)?;
            st.sessions.set(room, caller, sessions.clone());
            let members = store::room_member_ids(conn, &room)?;
            st.hub.publish(
                &members,
                &Event::Sessions {
                    room,
                    member: caller,
                    sessions,
                },
            );
            Ok(())
        })
        .await?;
    Ok(Response::Ok)
}
