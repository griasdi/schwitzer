//! A room from creation to a finished task, over a real server on loopback.

use std::net::SocketAddr;
use std::time::Duration;

use schwitzer_client::{Client, ClientError, ClientEvent};
use schwitzer_proto::{
    ApiError, Decision, MemberId, Priority, Project, Request, Response, RoomId, RoomKey, RoomState,
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
        // Tests never reach GitHub.
        releases_url: None,
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

fn project(response: Response) -> Project {
    match response {
        Response::Project(project) => project,
        other => panic!("expected a project, got {other:?}"),
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

    // 2. A creates the projects "app" and "site"; B sees them. "site" gets an accepted High
    // task, numbered before the ones of "app".
    let app = project(
        a.request(
            room,
            Request::CreateProject {
                room,
                name: "app".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(app.id, 1);
    room_where(&mut rx_b, |s| s.projects.contains(&app)).await;
    let site = project(
        a.request(
            room,
            Request::CreateProject {
                room,
                name: "site".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(site.id, 2);
    let site_task = task(
        a.request(
            room,
            Request::Propose {
                room,
                project: site.id,
                title: "new landing page".into(),
                body: String::new(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(site_task.project, site.id);
    for request in [
        Request::SetPriority {
            room,
            task: site_task.number,
            priority: Priority::High,
        },
        Request::Decide {
            room,
            task: site_task.number,
            decision: Decision::Accept,
        },
    ] {
        task(a.request(room, request).await.unwrap());
    }

    // Two proposals in "app", the second made High.
    let normal = task(
        a.request(
            room,
            Request::Propose {
                room,
                project: app.id,
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
                project: app.id,
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
                project: None,
                task: Some(normal.number),
                agent: "bob-agent".into(),
            },
        )
        .await;
    assert_eq!(api_error(refused), ApiError::WrongStage(Stage::Proposed));

    // 4. An idea is written up into a proposal, once.
    let idea = task(
        a.request(
            room,
            Request::AddIdea {
                room,
                project: app.id,
                title: "faster sync".into(),
                body: String::new(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(idea.stage, Stage::Idea);
    let write_up = Request::ProposeIdea {
        room,
        task: idea.number,
        title: "Sync rooms in parallel".into(),
        body: "why".into(),
    };
    let proposal = task(b.request(room, write_up.clone()).await.unwrap());
    assert_eq!(proposal.stage, Stage::Proposed);
    assert_eq!(proposal.number, idea.number);
    assert_eq!(proposal.title, "Sync rooms in parallel");
    assert_eq!(proposal.author, idea.author);
    assert_eq!(
        api_error(b.request(room, write_up).await),
        ApiError::WrongStage(Stage::Proposed)
    );

    // 5. After both are accepted, B's next plan claim in "app" is its High one, not the
    // older High one of "site".
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
                project: Some(app.id),
                task: None,
                agent: "bob-agent".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(plan_claim.number, urgent.number);
    assert_eq!(plan_claim.claim.as_ref().unwrap().member, b.member());

    // The "site" task moves to "app"; "site" is renamed to "web", whose name then is taken
    // in any case.
    let moved = task(
        a.request(
            room,
            Request::MoveTask {
                room,
                task: site_task.number,
                project: app.id,
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(moved.project, app.id);
    let web = project(
        a.request(
            room,
            Request::RenameProject {
                room,
                project: site.id,
                name: "web".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(web.id, site.id);
    let taken = b
        .request(
            room,
            Request::CreateProject {
                room,
                name: "WEB".into(),
            },
        )
        .await;
    assert_eq!(api_error(taken), ApiError::ProjectExists);

    // 6. A cannot submit a plan on B's claim.
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

    // 7. B submits the plan, A approves it.
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

    // 8. B builds and finishes with a note; A sees it done.
    let build_claim = claimed(
        b.request(
            room,
            Request::Claim {
                room,
                work: Work::Build,
                project: None,
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

    // 9. The server restarts on its port with the same data: the key still joins and the
    // projects and tasks are still there.
    let port = addr.port;
    server.shutdown().await;
    let mut rx_b = b.subscribe();
    let listen: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let server = start_server(listen, &data).await;
    assert_eq!(server.addr, addr);
    assert_eq!(b.join_room(key.clone()).await.unwrap(), room);
    let state = room_where(&mut rx_b, |s| {
        s.tasks.len() == 4
            && s.tasks
                .iter()
                .any(|t| t.number == urgent.number && t.stage == Stage::Done)
    })
    .await;
    assert_eq!(state.id, room);
    assert_eq!(state.projects, [app.clone(), web]);
    let moved = state.tasks.iter().find(|t| t.number == site_task.number);
    assert_eq!(moved.map(|t| t.project), Some(app.id));
    let kept: Vec<RoomKey> = b.rooms().into_iter().map(|(key, _)| key).collect();
    assert_eq!(kept, [key]);

    a.shutdown();
    b.shutdown();
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forgotten_room_closes_its_server_connection() {
    let data = TempDir::new().unwrap();
    let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server = start_server(listen, &data).await;
    let addr: ServerAddr = server.addr.clone();

    let (dir_a, dir_b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let a = Client::start(dir_a.path(), "alice".into()).unwrap();
    let b = Client::start(dir_b.path(), "bob".into()).unwrap();
    let (mut rx_a, mut rx_b) = (a.subscribe(), b.subscribe());

    let key = a.create_room(addr, "crew".into()).await.unwrap();
    let room = RoomId::of(&key.token);
    assert_eq!(b.join_room(key.clone()).await.unwrap(), room);
    let alice_online = |online: bool| {
        let alice = a.member();
        move |s: &RoomState| {
            s.members
                .iter()
                .any(|m| m.id == alice && m.online == online)
        }
    };
    room_where(&mut rx_b, alice_online(true)).await;

    // A forgets the room: no room is kept, and the server sees A's connection end while A
    // stays a member.
    a.forget_room(room);
    assert!(a.rooms().is_empty());
    let state = room_where(&mut rx_b, alice_online(false)).await;
    assert!(has_member(&state, a.member()));

    // The key brings A back on a fresh connection to the same server.
    a.add_room(key.clone());
    room_where(&mut rx_a, |s| s.id == room).await;
    room_where(&mut rx_b, alice_online(true)).await;

    a.shutdown();
    b.shutdown();
    server.shutdown().await;
}

/// Serves `body` as JSON to every HTTP request; returns the URL of the release list.
fn serve_feed(body: String) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut buf = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://127.0.0.1:{port}/releases")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_announces_the_newest_release_not_above_its_own_version() {
    let own = env!("CARGO_PKG_VERSION");
    let release = |tag: &str| format!(r#"{{"tag_name":"{tag}","draft":false,"prerelease":false}}"#);
    let body = format!(
        "[{},{},{}]",
        release("v999.0.0"),
        release(&format!("v{own}")),
        release("v0.0.1")
    );
    let data = TempDir::new().unwrap();
    let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server = schwitzer_server::start(Options {
        releases_url: Some(serve_feed(body)),
        ..options(listen, &data)
    })
    .await
    .unwrap();
    let addr: ServerAddr = server.addr.clone();

    let dir = TempDir::new().unwrap();
    let a = Client::start(dir.path(), "alice".into()).unwrap();
    let mut rx = a.subscribe();
    a.create_room(addr.clone(), "crew".into()).await.unwrap();
    let (server_of, version) = tokio::time::timeout(WAIT, async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Release { server, version }) => return (server, version),
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => panic!("the event channel closed"),
            }
        }
    })
    .await
    .expect("no release announced");
    assert_eq!(version, own);
    assert_eq!(server_of, addr);

    a.shutdown();
    server.shutdown().await;
}
