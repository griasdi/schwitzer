//! A room from creation to a finished task, over a real server on loopback.

use std::net::SocketAddr;
use std::time::Duration;

use schwitzer_client::{Client, ClientError, ClientEvent};
use schwitzer_proto::{
    ApiError, Decision, MemberId, Priority, Project, Request, Response, RoomId, RoomKey, RoomState,
    ServerAddr, Stage, Task, Work, caps,
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
                waits_for: vec![],
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
                waits_for: vec![],
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
                waits_for: vec![],
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
        waits_for: vec![],
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

#[tokio::test(flavor = "multi_thread")]
async fn a_member_who_sends_too_fast_is_told_to_wait() {
    let data = TempDir::new().unwrap();
    let server = start_server("127.0.0.1:0".parse().unwrap(), &data).await;
    let dir = TempDir::new().unwrap();
    let a = Client::start(dir.path(), "alice".into()).unwrap();

    let key = a
        .create_room(server.addr.clone(), "crew".into())
        .await
        .unwrap();
    let room = RoomId::of(&key.token);
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
    let proposal = task(
        a.request(
            room,
            Request::Propose {
                room,
                project: app.id,
                title: "tidy up".into(),
                body: String::new(),
                waits_for: vec![],
            },
        )
        .await
        .unwrap(),
    );
    let set_priority = || Request::SetPriority {
        room,
        task: proposal.number,
        priority: Priority::High,
    };

    let mut sent = 0;
    let refused = loop {
        assert!(sent < 200, "never refused");
        match a.request(room, set_priority()).await {
            Ok(_) => sent += 1,
            Err(e) => break api_error(Err(e)),
        }
    };
    assert_eq!(
        refused,
        ApiError::Invalid("too many requests, try again in 1 s".into())
    );
    assert!(sent >= 57, "only {sent} requests before the refusal");

    tokio::time::sleep(Duration::from_millis(1100)).await;
    task(a.request(room, set_priority()).await.unwrap());

    a.shutdown();
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_room_refuses_more_and_its_snapshot_still_arrives() {
    let data = TempDir::new().unwrap();
    let server = start_server("127.0.0.1:0".parse().unwrap(), &data).await;
    let (dir_a, dir_b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let a = Client::start(dir_a.path(), "alice".into()).unwrap();
    let b = Client::start(dir_b.path(), "bob".into()).unwrap();
    let mut rx_b = b.subscribe();

    let key = a
        .create_room(server.addr.clone(), "crew".into())
        .await
        .unwrap();
    let room = RoomId::of(&key.token);
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
    let mut refused = None;
    for i in 0..20 {
        let proposal = task(
            a.request(
                room,
                Request::Propose {
                    room,
                    project: app.id,
                    title: format!("task {i}"),
                    body: String::new(),
                    waits_for: vec![],
                },
            )
            .await
            .unwrap(),
        );
        let edit = Request::EditTask {
            room,
            task: proposal.number,
            title: proposal.title.clone(),
            body: String::new(),
            plan: "x".repeat(caps::PLAN),
        };
        if let Err(e) = a.request(room, edit).await {
            refused = Some((i, api_error(Err(e)), proposal.number));
            break;
        }
    }
    let (i, error, number) = refused.expect("the room never filled up");
    assert_eq!(i, 15);
    assert_eq!(
        error,
        ApiError::Invalid(
            "the room is full: its tasks, notes, members and sessions must stay under 4 MiB".into()
        )
    );

    assert_eq!(b.join_room(key).await.unwrap(), room);
    let state = room_where(&mut rx_b, |s| s.tasks.len() == 16).await;
    let last = state.tasks.iter().find(|t| t.number == number).unwrap();
    assert!(last.plan.is_empty());

    a.shutdown();
    b.shutdown();
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_project_is_deleted_for_every_member() {
    let data = TempDir::new().unwrap();
    let server = start_server("127.0.0.1:0".parse().unwrap(), &data).await;
    let (dir_a, dir_b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let a = Client::start(dir_a.path(), "alice".into()).unwrap();
    let b = Client::start(dir_b.path(), "bob".into()).unwrap();
    let mut rx_b = b.subscribe();

    let key = a
        .create_room(server.addr.clone(), "crew".into())
        .await
        .unwrap();
    let room = RoomId::of(&key.token);
    assert_eq!(b.join_room(key).await.unwrap(), room);
    let create = |name: &str| Request::CreateProject {
        room,
        name: name.into(),
    };
    let app = project(a.request(room, create("app")).await.unwrap());
    let old = project(a.request(room, create("old")).await.unwrap());
    let propose = |project: u64| Request::Propose {
        room,
        project,
        title: "a task".into(),
        body: String::new(),
        waits_for: vec![],
    };
    a.request(room, propose(app.id)).await.unwrap();
    room_where(&mut rx_b, |s| {
        s.projects == [app.clone(), old.clone()] && s.tasks.len() == 1
    })
    .await;

    let delete = |project: u64| Request::DeleteProject { room, project };
    assert_eq!(
        api_error(a.request(room, delete(app.id)).await),
        ApiError::ProjectNotEmpty
    );
    assert_eq!(a.request(room, delete(old.id)).await.unwrap(), Response::Ok);
    room_where(&mut rx_b, |s| s.projects == [app.clone()]).await;
    assert_eq!(
        api_error(b.request(room, delete(old.id)).await),
        ApiError::NoSuchProject
    );
    assert_eq!(
        api_error(b.request(room, propose(old.id)).await),
        ApiError::NoSuchProject
    );

    a.shutdown();
    b.shutdown();
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_that_waits_is_built_after_its_prerequisite() {
    let data = TempDir::new().unwrap();
    let server = start_server("127.0.0.1:0".parse().unwrap(), &data).await;
    let (dir_a, dir_b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let a = Client::start(dir_a.path(), "alice".into()).unwrap();
    let b = Client::start(dir_b.path(), "bob".into()).unwrap();
    let mut rx_b = b.subscribe();

    let key = a
        .create_room(server.addr.clone(), "crew".into())
        .await
        .unwrap();
    let room = RoomId::of(&key.token);
    assert_eq!(b.join_room(key).await.unwrap(), room);
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
    let propose = |title: &str, waits_for: Vec<u64>| Request::Propose {
        room,
        project: app.id,
        title: title.into(),
        body: String::new(),
        waits_for,
    };
    let base = task(a.request(room, propose("base", vec![])).await.unwrap());
    let top = task(
        a.request(room, propose("top", vec![base.number]))
            .await
            .unwrap(),
    );
    assert_eq!(top.waits_for, [base.number]);
    let other = task(a.request(room, propose("other", vec![])).await.unwrap());
    for n in [base.number, top.number, other.number] {
        let decide = |decision| Request::Decide {
            room,
            task: n,
            decision,
        };
        a.request(room, decide(Decision::Accept)).await.unwrap();
        let claim = Request::Claim {
            room,
            work: Work::Plan,
            project: None,
            task: Some(n),
            agent: "planner".into(),
        };
        claimed(a.request(room, claim).await.unwrap());
        let plan = Request::SubmitPlan {
            room,
            task: n,
            plan: "do it".into(),
        };
        a.request(room, plan).await.unwrap();
        a.request(room, decide(Decision::Approve)).await.unwrap();
    }
    let state = room_where(&mut rx_b, |s| {
        s.tasks.len() == 3 && s.tasks.iter().all(|t| t.stage == Stage::Ready)
    })
    .await;
    let seen = state.tasks.iter().find(|t| t.number == top.number).unwrap();
    assert_eq!(seen.waits_for, [base.number]);

    let build = |task: Option<u64>| Request::Claim {
        room,
        work: Work::Build,
        project: None,
        task,
        agent: "builder".into(),
    };
    // `top` waits for `base`, so the next free builds are `base` and `other`, then none.
    assert_eq!(
        claimed(a.request(room, build(None)).await.unwrap()).number,
        base.number
    );
    assert_eq!(
        claimed(a.request(room, build(None)).await.unwrap()).number,
        other.number
    );
    assert_eq!(
        a.request(room, build(None)).await.unwrap(),
        Response::Claimed(None)
    );
    assert_eq!(
        api_error(a.request(room, build(Some(top.number))).await),
        ApiError::Waiting(base.number)
    );
    let finish = Request::Finish {
        room,
        task: base.number,
        note: "built".into(),
    };
    a.request(room, finish).await.unwrap();
    assert_eq!(
        claimed(a.request(room, build(None)).await.unwrap()).number,
        top.number
    );

    let set_waits = |task: u64, waits_for: Vec<u64>| Request::SetWaits {
        room,
        task,
        waits_for,
    };
    let other_waits = task(
        a.request(room, set_waits(other.number, vec![top.number]))
            .await
            .unwrap(),
    );
    assert_eq!(other_waits.waits_for, [top.number]);
    match api_error(
        a.request(room, set_waits(top.number, vec![other.number]))
            .await,
    ) {
        ApiError::Invalid(text) => assert!(text.contains("already waits for"), "{text}"),
        other => panic!("expected Invalid, got {other:?}"),
    }

    a.shutdown();
    b.shutdown();
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_is_in_at_most_50_rooms() {
    let data = TempDir::new().unwrap();
    let server = start_server("127.0.0.1:0".parse().unwrap(), &data).await;
    let addr = server.addr.clone();
    let (dir_a, dir_b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let a = Client::start(dir_a.path(), "alice".into()).unwrap();
    let b = Client::start(dir_b.path(), "bob".into()).unwrap();
    let too_many = ApiError::Invalid(
        "you are in 50 rooms already; leave one to create or join another".into(),
    );
    let refusal = |result: Result<_, ClientError>| match result {
        Err(ClientError::Api(e)) => e,
        other => panic!("expected an API error, got {other:?}"),
    };

    let mut first = None;
    for i in 0..50 {
        let key = a
            .create_room(addr.clone(), format!("room {i}"))
            .await
            .unwrap();
        first.get_or_insert(RoomId::of(&key.token));
    }
    assert_eq!(
        refusal(
            a.create_room(addr.clone(), "one more".into())
                .await
                .map(|_| ())
        ),
        too_many
    );

    let key_b = b.create_room(addr.clone(), "bob's".into()).await.unwrap();
    assert_eq!(
        refusal(a.join_room(key_b.clone()).await.map(|_| ())),
        too_many
    );

    a.leave_room(first.unwrap()).await.unwrap();
    assert_eq!(
        a.join_room(key_b.clone()).await.unwrap(),
        RoomId::of(&key_b.token)
    );

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
async fn the_server_announces_the_newest_release_even_above_its_own_version() {
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
    assert_eq!(version, "999.0.0");
    assert_eq!(server_of, addr);

    a.shutdown();
    server.shutdown().await;
}
