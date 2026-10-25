//! The schwitzer client library: runs on its own runtime, keeps one connection per server
//! the user's rooms live on, mirrors the room states and routes requests to the right
//! server.

mod backoff;
mod conn;
mod sync;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use schwitzer_proto::{
    ApiError, DebugId, DebugInstruction, DebugSession, MemberId, Request, Response, RoomId,
    RoomKey, RoomState, ServerAddr,
};
use tokio::sync::broadcast;
use weida::{FilesOptions, IdentitySource};

use crate::conn::ServerConn;

/// Capacity of the event channel.
const EVENT_CAPACITY: usize = 1024;
/// Debug instructions kept per room; the oldest go first.
pub(crate) const INBOX_CAP: usize = 100;

/// Errors of the client library.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    /// There is no connection to the server.
    #[error("server is offline")]
    Offline,
    /// The server did not answer in time.
    #[error("timed out")]
    Timeout,
    /// The server refused the request.
    #[error("{0}")]
    Api(ApiError),
    /// The network or the local identity failed.
    #[error("network: {0}")]
    Net(String),
    /// A message could not be encoded or decoded.
    #[error("protocol: {0}")]
    Proto(String),
}

impl From<ApiError> for ClientError {
    fn from(e: ApiError) -> Self {
        Self::Api(e)
    }
}

impl From<weida::Error> for ClientError {
    fn from(e: weida::Error) -> Self {
        Self::Net(e.to_string())
    }
}

impl From<schwitzer_proto::ProtoError> for ClientError {
    fn from(e: schwitzer_proto::ProtoError) -> Self {
        Self::Proto(e.to_string())
    }
}

impl From<schwitzer_proto::FrameError> for ClientError {
    fn from(e: schwitzer_proto::FrameError) -> Self {
        Self::Proto(e.to_string())
    }
}

/// What the client tells the application.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    /// A server came online or went offline, or has something to report about its rooms.
    Status {
        server: ServerAddr,
        online: bool,
        error: Option<String>,
    },
    /// A room has a new state.
    Room(Box<RoomState>),
    /// The room is no longer kept: the user left it, or it was forgotten on this machine.
    RoomRemoved(RoomId),
    /// The newest app release, as `server` announced it (unchecked text).
    Release { server: ServerAddr, version: String },
    /// Log bytes of `from`'s file `stream` in the room's debug session, starting at byte
    /// `offset` of that file.
    DebugLog {
        room: RoomId,
        session: DebugId,
        from: MemberId,
        stream: String,
        offset: u64,
        data: Arc<[u8]>,
    },
    /// The room's debug inbox ([`Client::debug_inbox`]) changed.
    DebugInbox { room: RoomId },
}

/// A debug instruction this client received: as an addressee, or as the master who sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEntry {
    pub instruction: DebugInstruction,
    /// This member is an addressee.
    pub for_me: bool,
    /// An omp session took it ([`Client::take_debug_instruction`]).
    pub taken: bool,
}

/// The state of a room and the server it lives on.
struct Entry {
    server: ServerAddr,
    state: RoomState,
}

struct Inner {
    member: MemberId,
    name: Mutex<String>,
    identity: IdentitySource,
    handle: tokio::runtime::Handle,
    rt: Mutex<Option<tokio::runtime::Runtime>>,
    events: broadcast::Sender<ClientEvent>,
    /// The rooms the user keeps, in the order they were added.
    keys: Mutex<Vec<RoomKey>>,
    rooms: Mutex<HashMap<RoomId, Entry>>,
    servers: Mutex<HashMap<ServerAddr, Arc<ServerConn>>>,
    /// Per room, the debug session's instructions this client received.
    debug_inbox: Mutex<HashMap<RoomId, (DebugId, Vec<InboxEntry>)>>,
    /// Per room, the debug session this member joined and joins again after a reconnect.
    debug_rejoin: Mutex<HashMap<RoomId, DebugId>>,
    closed: AtomicBool,
}

/// A schwitzer client; clones share one connection set.
#[derive(Clone)]
pub struct Client(Arc<Inner>);

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Client {
    /// Starts a client on its own runtime, with the identity in `identity_dir` (created on
    /// first use) and `name` as its display name. It connects to servers as rooms are
    /// added.
    pub fn start(identity_dir: &Path, name: String) -> Result<Self, ClientError> {
        let identity = IdentitySource::files(identity_dir, FilesOptions::default())?;
        let member = MemberId(*identity.fingerprint()?.as_bytes());
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("schwitzer-net")
            .enable_all()
            .build()
            .map_err(|e| ClientError::Net(e.to_string()))?;
        let handle = rt.handle().clone();
        Ok(Self(Arc::new(Inner {
            member,
            name: Mutex::new(name),
            identity,
            handle,
            rt: Mutex::new(Some(rt)),
            events: broadcast::channel(EVENT_CAPACITY).0,
            keys: Mutex::default(),
            rooms: Mutex::default(),
            servers: Mutex::default(),
            debug_inbox: Mutex::default(),
            debug_rejoin: Mutex::default(),
            closed: AtomicBool::new(false),
        })))
    }

    /// The identity of this client on every server.
    pub fn member(&self) -> MemberId {
        self.0.member
    }

    /// The events from now on. A receiver that lags loses events: resync from
    /// [`Client::rooms`].
    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.0.events.subscribe()
    }

    /// The rooms the user keeps with their last known state.
    pub fn rooms(&self) -> Vec<(RoomKey, Option<RoomState>)> {
        let keys = lock(&self.0.keys).clone();
        let rooms = lock(&self.0.rooms);
        keys.into_iter()
            .map(|key| {
                let state = rooms
                    .get(&RoomId::of(&key.token))
                    .map(|entry| entry.state.clone());
                (key, state)
            })
            .collect()
    }

    /// Keeps `key`: its server is connected and the room joined, now and after every
    /// reconnect.
    pub fn add_room(&self, key: RoomKey) {
        let conn = self.remember(key.clone());
        if conn.session_now().is_some() {
            let client = self.clone();
            self.0
                .handle
                .spawn(async move { client.join_late(&conn).await });
        }
    }

    /// Creates a room on `server` and keeps its key.
    pub async fn create_room(
        &self,
        server: ServerAddr,
        name: String,
    ) -> Result<RoomKey, ClientError> {
        let conn = self.connection(&server)?;
        match conn.rpc(&Request::CreateRoom { name }).await? {
            Response::Room { token, .. } => {
                let key = RoomKey { server, token };
                self.remember(key.clone()).mark_joined(key.token);
                Ok(key)
            }
            other => Err(unexpected(&other)),
        }
    }

    /// Joins the room of `key` and keeps the key.
    pub async fn join_room(&self, key: RoomKey) -> Result<RoomId, ClientError> {
        let conn = self.connection(&key.server)?;
        match conn.rpc(&Request::JoinRoom { token: key.token }).await? {
            Response::Room { id, .. } => {
                self.remember(key.clone()).mark_joined(key.token);
                Ok(id)
            }
            other => Err(unexpected(&other)),
        }
    }

    /// Leaves the room and forgets its key. A room the server no longer knows is
    /// forgotten as well.
    pub async fn leave_room(&self, room: RoomId) -> Result<(), ClientError> {
        match self.request(room, Request::LeaveRoom { room }).await {
            Ok(_) | Err(ClientError::Api(ApiError::NotMember | ApiError::NoSuchRoom)) => {
                self.forget_room(room);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Sends `req` to the server `room` lives on and waits for the reply, for up to
    /// `SESSION_WAIT` for a connection and then `RPC_TIMEOUT` for the answer.
    pub async fn request(&self, room: RoomId, req: Request) -> Result<Response, ClientError> {
        let server = server_of(&lock(&self.0.keys), room)
            .cloned()
            .ok_or(ClientError::Api(ApiError::NoSuchRoom))?;
        self.connection(&server)?.rpc(&req).await
    }

    /// Changes the display name, on every server that is online now and on every later
    /// connection.
    pub async fn set_name(&self, name: String) {
        name.clone_into(&mut lock(&self.0.name));
        let sessions: Vec<_> = lock(&self.0.servers)
            .values()
            .filter_map(|conn| conn.session_now())
            .collect();
        let req = Request::SetName { name };
        let sent = sessions.iter().map(|session| session.call(&req));
        for result in futures::future::join_all(sent).await {
            if let Err(e) = result {
                tracing::warn!("setting the name failed: {e}");
            }
        }
    }

    /// Starts a debug session in `room` with this member as master.
    pub async fn debug_start(&self, room: RoomId) -> Result<DebugSession, ClientError> {
        self.enter_debug(room, Request::DebugStart { room }).await
    }

    /// Joins the running debug session of `room`.
    pub async fn debug_join(&self, room: RoomId) -> Result<DebugSession, ClientError> {
        let session = self.debug_id(room)?;
        self.enter_debug(room, Request::DebugJoin { room, session })
            .await
    }

    /// Sends `req`, which puts this member into the debug session of `room`, and remembers
    /// the session to rejoin after a reconnect.
    async fn enter_debug(&self, room: RoomId, req: Request) -> Result<DebugSession, ClientError> {
        match self.request(room, req).await? {
            Response::Debug(session) => {
                lock(&self.0.debug_rejoin).insert(room, session.id);
                Ok(session)
            }
            other => Err(unexpected(&other)),
        }
    }

    /// Leaves the debug session of `room`; as its master, ends it.
    pub async fn debug_leave(&self, room: RoomId) -> Result<(), ClientError> {
        lock(&self.0.debug_rejoin).remove(&room);
        let session = self.debug_id(room)?;
        self.request(room, Request::DebugLeave { room, session })
            .await
            .map(drop)
    }

    /// Sends bytes of this member's file `stream`, starting at byte `offset` of it.
    pub async fn debug_log(
        &self,
        room: RoomId,
        session: DebugId,
        stream: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<(), ClientError> {
        let req = Request::DebugLog {
            room,
            session,
            stream,
            offset,
            data,
        };
        self.request(room, req).await.map(drop)
    }

    /// As master, sends an instruction to `to` (empty: every other participant).
    pub async fn debug_instruct(
        &self,
        room: RoomId,
        to: Vec<MemberId>,
        text: String,
    ) -> Result<DebugInstruction, ClientError> {
        let session = self.debug_id(room)?;
        let req = Request::DebugInstruct {
            room,
            session,
            to,
            text,
        };
        match self.request(room, req).await? {
            Response::DebugInstruction(instruction) => Ok(instruction),
            other => Err(unexpected(&other)),
        }
    }

    /// The debug instructions received in `room`'s current session, oldest first.
    pub fn debug_inbox(&self, room: RoomId) -> Vec<InboxEntry> {
        lock(&self.0.debug_inbox)
            .get(&room)
            .map(|(_, entries)| entries.clone())
            .unwrap_or_default()
    }

    /// Takes the oldest instruction for this member that nobody took yet.
    pub fn take_debug_instruction(&self, room: RoomId) -> Option<DebugInstruction> {
        let taken = {
            let mut inboxes = lock(&self.0.debug_inbox);
            let (_, entries) = inboxes.get_mut(&room)?;
            let entry = entries.iter_mut().find(|e| e.for_me && !e.taken)?;
            entry.taken = true;
            entry.instruction.clone()
        };
        self.emit(ClientEvent::DebugInbox { room });
        Some(taken)
    }

    /// The id of `room`'s running debug session.
    fn debug_id(&self, room: RoomId) -> Result<DebugId, ClientError> {
        lock(&self.0.rooms)
            .get(&room)
            .and_then(|entry| entry.state.debug.as_ref().map(|s| s.id))
            .ok_or(ClientError::Api(ApiError::NoDebugSession))
    }

    /// Stops every connection and the runtime. The client is unusable afterwards.
    pub fn shutdown(&self) {
        if self.0.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        for conn in lock(&self.0.servers).values() {
            conn.stop();
        }
        if let Some(rt) = lock(&self.0.rt).take() {
            rt.shutdown_background();
        }
    }

    pub(crate) fn emit(&self, event: ClientEvent) {
        let _ = self.0.events.send(event);
    }

    fn name(&self) -> String {
        lock(&self.0.name).clone()
    }

    /// Keeps `key` once and makes sure its server is connected.
    fn remember(&self, key: RoomKey) -> Arc<ServerConn> {
        let server = key.server.clone();
        {
            let mut keys = lock(&self.0.keys);
            if !keys.iter().any(|k| k.token == key.token) {
                keys.push(key);
            }
        }
        self.ensure_server(&server)
    }

    /// The connection to `server`, started if need be; `Offline` after shutdown.
    fn connection(&self, server: &ServerAddr) -> Result<Arc<ServerConn>, ClientError> {
        if self.0.closed.load(Ordering::SeqCst) {
            return Err(ClientError::Offline);
        }
        Ok(self.ensure_server(server))
    }

    /// Drops the debug inbox and the rejoin wish of a room that is gone from this machine.
    fn forget_debug(&self, room: RoomId) {
        lock(&self.0.debug_inbox).remove(&room);
        lock(&self.0.debug_rejoin).remove(&room);
    }

    /// Forgets the room on this machine without telling its server: its key and its state.
    /// The connection to its server stops when no other kept room lives there. Tells the
    /// application if anything changed.
    pub fn forget_room(&self, room: RoomId) {
        let (key_gone, idle) = {
            let mut keys = lock(&self.0.keys);
            let server = server_of(&keys, room).cloned();
            keys.retain(|key| RoomId::of(&key.token) != room);
            let key_gone = server.is_some();
            let idle = server.filter(|server| !keys.iter().any(|key| key.server == *server));
            (key_gone, idle)
        };
        let state_gone = lock(&self.0.rooms).remove(&room).is_some();
        self.forget_debug(room);
        if let Some(server) = idle {
            let conn = lock(&self.0.servers).remove(&server);
            if let Some(conn) = conn {
                conn.stop();
            }
        }
        if key_gone || state_gone {
            self.emit(ClientEvent::RoomRemoved(room));
        }
    }
}

/// The server the kept room `room` lives on; `None` when no key of `keys` is that room's.
fn server_of(keys: &[RoomKey], room: RoomId) -> Option<&ServerAddr> {
    keys.iter()
        .find(|key| RoomId::of(&key.token) == room)
        .map(|key| &key.server)
}

fn unexpected(response: &Response) -> ClientError {
    ClientError::Proto(format!("unexpected reply {response:?}"))
}
