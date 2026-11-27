//! Everything a scenario runs against: a room server, bob (a second member who makes the human
//! decisions), the real headless app of alice with the plugin installed in a repo, and the model.

use std::fs;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use schwitzer_client::Client;
use schwitzer_proto::{Decision, Request, Response, RoomId, RoomState, Task, Work};
use schwitzer_server::{Options, Server};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use crate::fake_model::FakeModel;
use crate::omp::{Omp, omp_bin};

const READY: Duration = Duration::from_secs(20);

pub struct World {
    pub rt: Runtime,
    dir: Option<TempDir>,
    pub repo: PathBuf,
    agent_dir: PathBuf,
    server: Option<Server>,
    pub bob: Client,
    pub room: RoomId,
    project: u64,
    app: Child,
    app_log: PathBuf,
    pub model: FakeModel,
    omps: u32,
}

/// Polls `check` every 100 ms until it gives a value; panics after `READY`.
fn poll<T>(what: &str, mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + READY;
    loop {
        if let Some(found) = check() {
            return found;
        }
        if Instant::now() > deadline {
            panic!("waited {} s in vain for {what}", READY.as_secs());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn git(repo: &PathBuf, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .stdout(Stdio::null())
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

impl World {
    pub fn start() -> World {
        omp_bin();
        // Never under this repository: omp reads `.omp/` from the ancestors of its working directory.
        let tmp = tempfile::Builder::new()
            .prefix("sw-e2e-")
            .tempdir()
            .expect("temp dir");
        let dir = tmp.path().canonicalize().expect("canonical temp dir");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let server = rt
            .block_on(schwitzer_server::start(Options {
                listen: "127.0.0.1:0".parse().expect("listen address"),
                public_host: "127.0.0.1".into(),
                data_dir: dir.join("server"),
                releases: None,
            }))
            .expect("start the server");

        let bob = Client::start(&dir.join("bob"), "bob".into()).expect("start bob");
        let key = rt
            .block_on(bob.create_room(server.addr.clone(), "crew".into()))
            .expect("create the room");
        let room = RoomId::of(&key.token);
        let project = match rt
            .block_on(bob.request(
                room,
                Request::CreateProject {
                    room,
                    name: "demo".into(),
                },
            ))
            .expect("create the project")
        {
            Response::Project(p) => p.id,
            other => panic!("expected a project, got {other:?}"),
        };

        let repo = dir.join("repo");
        fs::create_dir_all(&repo).expect("repo dir");
        git(&repo, &["init", "-q"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "init",
            ],
        );
        // A placeholder: the plugin counts as installed but outdated, so the app's auto-update
        // writes the real one.
        let plugin = repo.join(".omp/plugins/node_modules/schwitzer");
        fs::create_dir_all(&plugin).expect("plugin dir");
        fs::write(plugin.join("schwitzer.json"), "{}").expect("plugin placeholder");

        let model = FakeModel::start(&rt);
        let agent_dir = dir.join("agent");
        fs::create_dir_all(&agent_dir).expect("agent dir");
        fs::write(agent_dir.join("models.yml"), model.models_yml()).expect("models.yml");

        let mcp_port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("free port")
            .port();
        let mut settings = toml::Table::new();
        settings.insert("name".into(), "alice".into());
        settings.insert("mcp_port".into(), i64::from(mcp_port).into());
        let sessions = agent_dir.join("sessions");
        settings.insert(
            "omp_sessions_dir".into(),
            sessions.to_str().expect("UTF-8 path").into(),
        );
        settings.insert("projects_dirs".into(), toml::Value::Array(vec![]));
        settings.insert(
            "rooms".into(),
            toml::Value::Array(vec![key.to_string().into()]),
        );
        settings.insert("auto_update_plugins".into(), true.into());
        let mut repos = toml::Table::new();
        repos.insert(
            format!("{}/{project}", room.to_hex()),
            toml::Value::Array(vec![repo.to_str().expect("UTF-8 path").into()]),
        );
        settings.insert("repos".into(), repos.into());
        let home = dir.join("app");
        fs::create_dir_all(&home).expect("app home");
        fs::write(home.join("settings.toml"), settings.to_string()).expect("settings.toml");

        let app_log = dir.join("app.log");
        let app = Command::new(env!("CARGO_BIN_EXE_schwitzer"))
            .arg("--headless")
            .arg("--control")
            .arg(dir.join("app.sock"))
            .args(["--size", "400x780", "--backend", "tiny-skia"])
            .env("SCHWITZER_HOME", &home)
            .env_remove("SCHWITZER_PICK_FOLDER")
            .env_remove("SCHWITZER_UPDATE_URL")
            .stdout(Stdio::null())
            .stderr(fs::File::create(&app_log).expect("app log"))
            .spawn()
            .expect("start the app");

        let world = World {
            rt,
            dir: Some(tmp),
            repo,
            agent_dir,
            server: Some(server),
            bob,
            room,
            project,
            app,
            app_log,
            model,
            omps: 0,
        };

        poll("the app's MCP server", || {
            TcpStream::connect(("127.0.0.1", mcp_port)).ok()
        });
        let source = include_str!("../../omp-plugin/index.ts");
        poll("the app to install the plugin", || {
            let index = fs::read_to_string(plugin.join("index.ts")).ok()?;
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(plugin.join("schwitzer.json")).ok()?)
                    .ok()?;
            (index == source && config["projects"].as_array()?.len() == 1).then_some(())
        });
        world.state_where("alice online in the room", |state| {
            state
                .members
                .iter()
                .any(|m| m.id != world.bob.member() && m.online)
        });
        world
    }

    /// Starts an omp in the repo; each gets its own stderr file in the temp dir.
    pub fn omp(&mut self) -> Omp {
        self.omps += 1;
        let stderr = self.path().join(format!("omp-{}.log", self.omps));
        Omp::start(&self.repo, &self.agent_dir, stderr)
    }

    fn path(&self) -> PathBuf {
        self.dir
            .as_ref()
            .expect("temp dir")
            .path()
            .canonicalize()
            .expect("canonical temp dir")
    }

    /// Bob's request in the room.
    fn call(&self, req: Request) -> Response {
        let what = format!("{req:?}");
        self.rt
            .block_on(self.bob.request(self.room, req))
            .unwrap_or_else(|e| panic!("{what}: {e}"))
    }

    fn decide(&self, task: u64, decision: Decision) {
        self.call(Request::Decide {
            room: self.room,
            task,
            decision,
        });
    }

    /// Proposes a task in the project and accepts it; returns its number.
    pub fn accepted(&self, title: &str, body: &str) -> u64 {
        let task = match self.call(Request::Propose {
            room: self.room,
            project: self.project,
            title: title.into(),
            body: body.into(),
            waits_for: vec![],
        }) {
            Response::Task(t) => t,
            other => panic!("expected a task, got {other:?}"),
        };
        self.decide(task.number, Decision::Accept);
        task.number
    }

    /// An accepted task with an approved plan, unclaimed: ready to build. Returns its number.
    pub fn approved(&self, title: &str, body: &str) -> u64 {
        let n = self.accepted(title, body);
        self.call(Request::Claim {
            room: self.room,
            work: Work::Plan,
            project: None,
            task: Some(n),
            agent: "test".into(),
        });
        self.call(Request::SubmitPlan {
            room: self.room,
            task: n,
            plan: format!("Plan of {title}."),
        });
        self.decide(n, Decision::Approve);
        n
    }

    /// Bob's state of the room once `pred` holds for it.
    pub fn state_where(&self, what: &str, pred: impl Fn(&RoomState) -> bool) -> RoomState {
        poll(what, || {
            self.bob
                .rooms()
                .into_iter()
                .filter_map(|(_, state)| state)
                .find(|state| state.id == self.room && pred(state))
        })
    }

    /// Task `n` once `pred` holds for it.
    pub fn task_where(&self, n: u64, what: &str, pred: impl Fn(&Task) -> bool) -> Task {
        let state = self.state_where(&format!("#{n} {what}"), |state| {
            state.task(n).is_some_and(&pred)
        });
        state.task(n).cloned().expect("the task")
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.app.kill().ok();
        self.app.wait().ok();
        self.bob.shutdown();
        drop(self.server.take());
        if std::thread::panicking()
            && let Some(dir) = self.dir.take()
        {
            let kept = dir.keep();
            let log = fs::read_to_string(&self.app_log).unwrap_or_default();
            let lines: Vec<&str> = log.lines().collect();
            let tail = lines[lines.len().saturating_sub(80)..].join("\n");
            eprintln!(
                "kept the test dir {}\nlast lines of app.log:\n{tail}",
                kept.display()
            );
        }
    }
}
