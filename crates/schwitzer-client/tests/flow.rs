//! A room from creation to a finished task, over a real server on loopback.

use std::net::SocketAddr;
use std::time::Duration;

use schwitzer_client::{Client, ClientError, ClientEvent};
use schwitzer_proto::{
    ApiError, Decision, MemberId, Priority, Request, Response, RoomId, RoomKey, RoomState,
    ServerAddr, Stage, Task, Work,
};
use schwitzer_server::{Options, Server};
use tempfile::TempDir;
use tokio::sync::broadcast::{Receiver, error::RecvError};

const WAIT: Duration = Duration::from_secs(20);

fn options(listen: SocketAddr, data: &TempDir) -> Options {
    Options {
        listen,
        public_host: "127.0.0.1".into(),
        data_dir: data.path().to_path_buf(),
    }
}

/// Starts the server again on `listen`, which the old one may take a moment to free.
async fn start_server(listen: SocketAddr, data: &TempDir) -> Server {
    let mut last = None;
    for _ in 0..50 {
        match schwitzer_server::start(options(listen, data)).await {
            Ok(server) => return server,
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not start: {:?}", last);
}

/// The next `Room` event for which `pred` holds.
async fn room_where(
    rx: &mut Receiver<ClientEvent>,
    pred: impl Fn(&RoomState) -> bool,
) -> RoomState {
    let wait = async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Room(state)) if pred(&state) => return *state,
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => panic!("the event channel closed"),
            }
        }
    };
    tokio::time::timeout(WAIT, wait)
        .await
        .expect("no room event matched in time")
}

fn task(response: Response) -> Task {
    match response {
        Response::Task(task) => task,
        other => panic!("expected a task, got {other:?}"),
    }
}

fn claimed(response: Response) -> Task {
    match response {
        Response::Claimed(Some(task)) => task,
        other => panic!("expected a claimed task, got {other:?}"),
    }
}

fn api_error(result: Result<Response, ClientError>) -> ApiError {
    match result {
        Err(ClientError::Api(e)) => e,
        other => panic!("expected an API error, got {other:?}"),
    }
}

fn has_member(state: &RoomState, member: MemberId) -> bool {
    state.members.iter().any(|m| m.id == member)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_goes_from_proposal_to_done_and_survives_a_server_restart() {
    let data = TempDir::new().unwrap();
    let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server = start_server(listen, &data).await;
    let addr: ServerAddr = server.addr.clone();

    let (dir_a, dir_b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let a = Client::start(dir_a.path(), "alice".into()).unwrap();
    let b = Client::start(dir_b.path(), "bob".into()).unwrap();
    let (mut rx_a, mut rx_b) = (a.subscribe(), b.subscribe());

    // 1. A creates a room, B joins it with the key and sees A in it.
    let key = a.create_room(addr.clone(), "crew".into()).await.unwrap();
    assert_eq!(key.server, addr);
    let room = RoomId::of(&key.token);
    assert_eq!(b.join_room(key.clone()).await.unwrap(), room);
    let state = room_where(&mut rx_b, |s| has_member(s, a.member())).await;
    assert_eq!(state.name, "crew");
    assert!(has_member(&state, b.member()));
    let alice = state.members.iter().find(|m| m.id == a.member()).unwrap();
    assert_eq!(alice.name, "alice");
    assert_eq!(a.rooms().len(), 1);
    assert_eq!(b.rooms()[0].0, key);

    // 2. Two proposals, the second made High.
    let normal = task(
        a.request(
            room,
            Request::Propose {
                room,
                title: "tidy up".into(),
                body: String::new(),
            },
        )
        .await
        .unwrap(),
    );
    let urgent = task(
        a.request(
            room,
            Request::Propose {
                room,
                title: "fix the build".into(),
                body: String::new(),
            },
        )
        .await
        .unwrap(),
    );
    task(
        a.request(
            room,
            Request::SetPriority {
                room,
                task: urgent.number,
                priority: Priority::High,
            },
        )
        .await
        .unwrap(),
    );

    // 3. A build claim on a proposal is refused.
    let refused = b
        .request(
            room,
            Request::Claim {
                room,
                work: Work::Build,
                task: Some(normal.number),
                agent: "bob-agent".into(),
            },
        )
        .await;
    assert_eq!(api_error(refused), ApiError::WrongStage(Stage::Proposed));

    // 4. After both are accepted, B's next plan claim is the High one.
    for number in [normal.number, urgent.number] {
        let accepted = task(
            a.request(
                room,
                Request::Decide {
                    room,
                    task: number,
                    decision: Decision::Accept,
                },
            )
            .await
            .unwrap(),
        );
        assert_eq!(accepted.stage, Stage::Accepted);
    }
    let plan_claim = claimed(
        b.request(
            room,
            Request::Claim {
                room,
                work: Work::Plan,
                task: None,
                agent: "bob-agent".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(plan_claim.number, urgent.number);
    assert_eq!(plan_claim.claim.as_ref().unwrap().member, b.member());

    // 5. A cannot submit a plan on B's claim.
    let stolen = a
        .request(
            room,
            Request::SubmitPlan {
                room,
                task: urgent.number,
                plan: "mine".into(),
            },
        )
        .await;
    assert_eq!(api_error(stolen), ApiError::NotYourClaim);

    // 6. B submits the plan, A approves it.
    let planned = task(
        b.request(
            room,
            Request::SubmitPlan {
                room,
                task: urgent.number,
                plan: "1. fix it".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(planned.stage, Stage::Planned);
    let approved = task(
        a.request(
            room,
            Request::Decide {
                room,
                task: urgent.number,
                decision: Decision::Approve,
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(approved.stage, Stage::Ready);

    // 7. B builds and finishes with a note; A sees it done.
    let build_claim = claimed(
        b.request(
            room,
            Request::Claim {
                room,
                work: Work::Build,
                task: None,
                agent: "bob-agent".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(build_claim.number, urgent.number);
    let done = task(
        b.request(
            room,
            Request::Finish {
                room,
                task: urgent.number,
                note: "shipped".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(done.stage, Stage::Done);
    assert!(done.notes.iter().any(|n| n.text == "shipped"));
    let seen = room_where(&mut rx_a, |s| {
        s.tasks
            .iter()
            .any(|t| t.number == urgent.number && t.stage == Stage::Done)
    })
    .await;
    let finished = seen
        .tasks
        .iter()
        .find(|t| t.number == urgent.number)
        .unwrap();
    assert!(finished.notes.iter().any(|n| n.text == "shipped"));

    // 8. The server restarts on its port with the same data: the key still joins and the
    // tasks are still there.
    let port = addr.port;
    server.shutdown().await;
    let mut rx_b = b.subscribe();
    let listen: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let server = start_server(listen, &data).await;
    assert_eq!(server.addr, addr);
    assert_eq!(b.join_room(key.clone()).await.unwrap(), room);
    let state = room_where(&mut rx_b, |s| {
        s.tasks.len() == 2
            && s.tasks
                .iter()
                .any(|t| t.number == urgent.number && t.stage == Stage::Done)
    })
    .await;
    assert_eq!(state.id, room);
    let kept: Vec<RoomKey> = b.rooms().into_iter().map(|(key, _)| key).collect();
    assert_eq!(kept, [key]);

    a.shutdown();
    b.shutdown();
    server.shutdown().await;
}
