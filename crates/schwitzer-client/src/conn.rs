//! One connection object per server: the requesters of `/rpc` and `/events`
//! of the current attempt, published through a watch so that requests can wait
//! for the next session.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use schwitzer_proto::{
    MAX_RPC_BYTES, PATH_EVENTS, PATH_RPC, RPC_TIMEOUT, Reply, Request, Response, RoomToken,
    ServerAddr, from_bytes, to_bytes,
};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use weida::{ClientTls, IdentitySource, ReconnectPolicy, Requester, RuntimeConfig, Trust};

use crate::{ClientError, lock};

/// Time allowed for dialling one endpoint.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a request waits for a server to come online.
const SESSION_WAIT: Duration = Duration::from_secs(10);
/// Longest wait of weida's own redial of a dropped connection.
const REDIAL_MAX: Duration = Duration::from_secs(5);

/// The weida runtime settings of every session.
fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        reconnect: ReconnectPolicy {
            max: REDIAL_MAX,
            ..ReconnectPolicy::default()
        },
        ..RuntimeConfig::default()
    }
}

/// The TLS terms of every dial: the address names the server key, our key
/// proves who we are.
fn tls(identity: &IdentitySource) -> ClientTls {
    ClientTls::new(Trust::by_address()).with_identity(identity.clone())
}

/// The live endpoints of one connection attempt to a server, on a weida runtime of its
/// own: a fresh UDP socket per attempt, so a socket that died (quinn ends its endpoint
/// on most recv errors, and weida never rebinds it) costs one attempt, not the process.
pub(crate) struct Session {
    pub(crate) events: Requester,
    rpc: Requester,
    addr: ServerAddr,
    wrt: weida::Runtime,
    handle: tokio::runtime::Handle,
    closed: AtomicBool,
}

impl Session {
    /// Dials `/rpc` and `/events`; a failed dial leaves nothing behind.
    pub(crate) async fn connect(
        handle: &tokio::runtime::Handle,
        identity: &IdentitySource,
        addr: ServerAddr,
    ) -> Result<Self, ClientError> {
        let wrt = weida::Runtime::with_handle(handle.clone(), runtime_config());
        let session = Self {
            events: wrt.requester(tls(identity)),
            rpc: wrt.requester(tls(identity)),
            addr,
            wrt,
            handle: handle.clone(),
            closed: AtomicBool::new(false),
        };
        if let Err(e) = session.dial().await {
            session.close();
            return Err(e);
        }
        Ok(session)
    }

    async fn dial(&self) -> Result<(), ClientError> {
        dial(self.rpc.connect(&self.addr.url(PATH_RPC))).await?;
        dial(self.events.connect(&self.addr.url(PATH_EVENTS))).await
    }

    /// One RPC exchange.
    pub(crate) async fn call(&self, req: &Request) -> Result<Response, ClientError> {
        let body = to_bytes(req);
        let exchange = async {
            let reply = self.rpc.request(&body).await?;
            reply.collect(MAX_RPC_BYTES).await
        };
        let bytes = tokio::time::timeout(RPC_TIMEOUT, exchange)
            .await
            .map_err(|_| ClientError::Timeout)??;
        from_bytes::<Reply>(&bytes)?.map_err(ClientError::Api)
    }

    /// Forgets both addresses, so the runtime stops redialling them, then shuts the
    /// runtime down: its socket and pooled connections close.
    pub(crate) fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        // Before the shutdown: a slot still registered would redial and bind a new
        // endpoint.
        self.rpc.disconnect(&self.addr.url(PATH_RPC));
        self.events.disconnect(&self.addr.url(PATH_EVENTS));
        let wrt = self.wrt.clone();
        self.handle.spawn(async move { wrt.shutdown().await });
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

async fn dial<F: Future<Output = Result<(), weida::Error>>>(f: F) -> Result<(), ClientError> {
    tokio::time::timeout(CONNECT_TIMEOUT, f)
        .await
        .map_err(|_| ClientError::Timeout)??;
    Ok(())
}

/// Everything the client knows about one server connection.
pub(crate) struct ServerConn {
    pub(crate) addr: ServerAddr,
    session: watch::Sender<Option<Arc<Session>>>,
    task: Mutex<Option<JoinHandle<()>>>,
    /// The tokens joined on the current session.
    pub(crate) joined: Mutex<HashSet<RoomToken>>,
    /// The status last emitted, so that a server that stays down is reported once.
    pub(crate) status: Mutex<Option<(bool, Option<String>)>>,
}

impl ServerConn {
    pub(crate) fn new(addr: ServerAddr) -> Self {
        Self {
            addr,
            session: watch::channel(None).0,
            task: Mutex::new(None),
            joined: Mutex::default(),
            status: Mutex::new(None),
        }
    }

    /// Notes that `token` is joined on the current session.
    pub(crate) fn mark_joined(&self, token: RoomToken) {
        lock(&self.joined).insert(token);
    }

    pub(crate) fn set_task(&self, task: JoinHandle<()>) {
        *lock(&self.task) = Some(task);
    }

    /// Publishes the current session; the one it replaces is closed.
    pub(crate) fn set_session(&self, session: Option<Arc<Session>>) {
        if let Some(old) = self.session.send_replace(session) {
            old.close();
        }
    }

    /// The current session without waiting for one.
    pub(crate) fn session_now(&self) -> Option<Arc<Session>> {
        self.session.borrow().clone()
    }

    /// The current session, waiting up to [`SESSION_WAIT`] for one.
    pub(crate) async fn session(&self) -> Result<Arc<Session>, ClientError> {
        let mut rx = self.session.subscribe();
        let guard = tokio::time::timeout(SESSION_WAIT, rx.wait_for(Option::is_some))
            .await
            .map_err(|_| ClientError::Offline)?
            .map_err(|_| ClientError::Offline)?;
        guard.clone().ok_or(ClientError::Offline)
    }

    /// One RPC exchange on the current session.
    pub(crate) async fn rpc(&self, req: &Request) -> Result<Response, ClientError> {
        self.session().await?.call(req).await
    }

    /// Aborts the connection task and drops the session.
    pub(crate) fn stop(&self) {
        if let Some(task) = lock(&self.task).take() {
            task.abort();
        }
        self.set_session(None);
    }
}
