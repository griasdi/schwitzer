//! schwitzer server: hosts the rooms, their tasks and who works on what, over
//! weida (QUIC).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use rusqlite::Connection;
use schwitzer_proto::{
    ApiError, Event, MAX_EVENT_FRAME, MAX_RPC_BYTES, Member, MemberId, PATH_EVENTS, PATH_RPC,
    RoomId, RoomState, ServerAddr, Task, encoded_len,
};
use tokio::task::JoinSet;
use weida::{
    Binding, ClientTrust, FilesOptions, IdentitySource, Listener, Runtime, RuntimeConfig, ServerTls,
};

mod auth;
mod events;
mod hub;
mod limit;
mod release;
mod rpc;
pub mod store;

use hub::{Hub, Sessions};
use store::{R, Store};

pub use release::RELEASES_URL;

/// How the server is run.
#[derive(Debug, Clone)]
pub struct Options {
    /// UDP address to listen on; port 0 picks a free one.
    pub listen: SocketAddr,
    /// Host name or address clients use to reach the server; it is also the
    /// name in the server's certificate.
    pub public_host: String,
    /// Directory for the database and the server's identity.
    pub data_dir: PathBuf,
    /// GitHub API URL of the release list, checked every five minutes; `None` checks nothing.
    pub releases_url: Option<String>,
}

/// What every request handler shares.
pub(crate) struct Shared {
    pub store: Store,
    pub hub: Hub,
    pub sessions: Sessions,
    /// The app release announced to every stream, once the release check found one.
    pub release: Mutex<Option<semver::Version>>,
    /// Request budget per member.
    pub limits: limit::Limits,
}

/// What a room snapshot (`Event::Room`) may take. Name changes are not checked; the 64 KiB
/// left of the frame cover them (at most 64 bytes per member and room).
const ROOM_BYTES: usize = MAX_EVENT_FRAME - 64 * 1024;
/// What a task may take: a reply wraps it in at most three enum tags and must fit
/// `MAX_RPC_BYTES`.
const TASK_BYTES: usize = MAX_RPC_BYTES - 16;

/// Refuses a room snapshot above [`ROOM_BYTES`].
pub(crate) fn require_room_fits(snapshot: &Event) -> Result<(), ApiError> {
    if encoded_len(snapshot) > ROOM_BYTES {
        return Err(ApiError::Invalid(
            "the room is full: its tasks, notes, members and sessions must stay under 4 MiB".into(),
        ));
    }
    Ok(())
}

/// Refuses a task above [`TASK_BYTES`].
pub(crate) fn require_task_fits(task: &Task) -> Result<(), ApiError> {
    if encoded_len(task) > TASK_BYTES {
        return Err(ApiError::Invalid(
            "the task is full: with its notes it must stay under 1 MiB".into(),
        ));
    }
    Ok(())
}

pub(crate) type State = Arc<Shared>;

impl Shared {
    /// The full state of a room: its stored members and tasks, with presence
    /// from the hub and the sessions held in memory.
    pub fn room_state(&self, conn: &Connection, room: &RoomId) -> R<RoomState> {
        let mut state = store::room_state(conn, room)?;
        for member in &mut state.members {
            member.online = self.hub.is_online(&member.id);
        }
        state.sessions = self.sessions.of_room(room);
        Ok(state)
    }

    /// Refuses when `room`'s snapshot, as it would be sent now (uncommitted changes of `conn`'s
    /// transaction included), exceeds [`ROOM_BYTES`].
    pub fn require_room_fits(&self, conn: &Connection, room: &RoomId) -> R<()> {
        Ok(require_room_fits(&Event::Room(
            self.room_state(conn, room)?,
        ))?)
    }

    /// Tells every room of `member` about their name and presence.
    pub fn announce(&self, conn: &Connection, member: &MemberId) -> R<()> {
        let info = Member {
            id: *member,
            name: store::person_name(conn, member)?,
            online: self.hub.is_online(member),
        };
        for room in store::rooms_of(conn, member)? {
            let ids = store::room_member_ids(conn, &room)?;
            self.hub.publish(
                &ids,
                &Event::Member {
                    room,
                    member: info.clone(),
                },
            );
        }
        Ok(())
    }
}

/// A running server. Dropping it stops the endpoints.
pub struct Server {
    /// How clients reach this server.
    pub addr: ServerAddr,
    tasks: JoinSet<&'static str>,
    binding: Binding,
    _listener: Listener,
    _runtime: Runtime,
}

impl Server {
    /// Waits until an endpoint task ends and returns its name. They serve until
    /// aborted, so one that ends (or panics) is a failure to exit on.
    pub async fn ended(&mut self) -> &'static str {
        match self.tasks.join_next().await {
            Some(Ok(name)) => name,
            Some(Err(e)) => {
                tracing::error!("endpoint task failed: {e}");
                "panicked task"
            }
            None => std::future::pending().await,
        }
    }

    /// Stops the endpoint tasks, closes the connections and waits for the
    /// socket to go idle, so the port can be bound again.
    pub async fn shutdown(mut self) {
        self.tasks.abort_all();
        while self.tasks.join_next().await.is_some() {}
        self.binding.close().await;
    }
}

/// Opens the store and the identity, binds the QUIC endpoint and starts serving.
pub async fn start(opts: Options) -> anyhow::Result<Server> {
    std::fs::create_dir_all(&opts.data_dir)
        .with_context(|| format!("creating {}", opts.data_dir.display()))?;
    let store = Store::open(&opts.data_dir.join("schwitzer.db"))?;
    let source = IdentitySource::files(
        opts.data_dir.join("identity"),
        FilesOptions {
            names: vec![opts.public_host.clone()],
            ..FilesOptions::default()
        },
    )
    .context("loading the server identity")?;
    let fingerprint = source
        .fingerprint()
        .context("server identity fingerprint")?;

    let runtime = Runtime::new(RuntimeConfig::default()).context("starting the weida runtime")?;
    let listener = runtime.listener();
    let binding = listener
        .bind_quic(
            opts.listen,
            ServerTls::new(source).require_client(ClientTrust::AnyKey),
        )
        .await
        .with_context(|| format!("binding {}", opts.listen))?;
    let rpc_replier = listener.replier(PATH_RPC)?;
    let events_replier = listener.replier(PATH_EVENTS)?;

    let state: State = Arc::new(Shared {
        store,
        hub: Hub::default(),
        sessions: Sessions::default(),
        release: Mutex::new(None),
        limits: limit::Limits::default(),
    });
    let mut tasks: JoinSet<&'static str> = JoinSet::new();
    macro_rules! spawn {
        ($name:literal, $task:expr) => {{
            let task = $task;
            tasks.spawn(async move {
                task.await;
                $name
            });
        }};
    }
    spawn!("rpc", rpc::serve(state.clone(), rpc_replier));
    spawn!("events", events::serve(state.clone(), events_replier));
    if let Some(url) = opts.releases_url {
        spawn!("release check", release::watch(state.clone(), url));
    }

    let addr = ServerAddr {
        host: opts.public_host,
        port: binding.local_addr().port(),
        fingerprint: *fingerprint.as_bytes(),
    };
    tracing::info!("schwitzer server listening on {}", binding.local_addr());
    Ok(Server {
        addr,
        tasks,
        binding,
        _listener: listener,
        _runtime: runtime,
    })
}
