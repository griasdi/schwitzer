//! The per-server task: dial, hello, join the kept rooms, then read the event stream and
//! apply it to the room states.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use schwitzer_proto::{
    ApiError, Event, MAX_EVENT_FRAME, Member, PROTOCOL_VERSION, Request, Response, RoomId, RoomKey,
    RoomState, ServerAddr, Subscribe, read_frame, to_bytes,
};
use weida::TransferMeta;

use crate::backoff::Backoff;
use crate::conn::{ServerConn, Session};
use crate::{Client, ClientError, ClientEvent, Entry, lock};

const FIRST_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// How long the server may stay silent while the stream opens and the backlog arrives.
const BACKLOG_STALL: Duration = Duration::from_secs(60);

/// `fut`, or [`ClientError::Timeout`] when it takes longer than [`BACKLOG_STALL`].
async fn within_stall<T, E: Into<ClientError>>(
    fut: impl Future<Output = Result<T, E>>,
) -> Result<T, ClientError> {
    tokio::time::timeout(BACKLOG_STALL, fut)
        .await
        .map_err(|_| ClientError::Timeout)?
        .map_err(Into::into)
}

impl Client {
    /// The connection to `addr`; its task is started on first use.
    pub(crate) fn ensure_server(&self, addr: &ServerAddr) -> Arc<ServerConn> {
        let mut servers = lock(&self.0.servers);
        if let Some(conn) = servers.get(addr) {
            return Arc::clone(conn);
        }
        let conn = Arc::new(ServerConn::new(addr.clone()));
        servers.insert(addr.clone(), Arc::clone(&conn));
        let client = self.clone();
        let task_conn = Arc::clone(&conn);
        conn.set_task(
            self.0
                .handle
                .spawn(async move { client.server_loop(task_conn).await }),
        );
        conn
    }

    fn set_status(&self, conn: &ServerConn, online: bool, error: Option<String>) {
        let status = (online, error);
        {
            let mut last = lock(&conn.status);
            if last.as_ref() == Some(&status) {
                return;
            }
            *last = Some(status.clone());
        }
        self.emit(ClientEvent::Status {
            server: conn.addr.clone(),
            online: status.0,
            error: status.1,
        });
    }

    async fn server_loop(&self, conn: Arc<ServerConn>) {
        let mut backoff = Backoff::new(FIRST_BACKOFF, MAX_BACKOFF);
        loop {
            let outcome =
                match Session::connect(&self.0.handle, &self.0.identity, conn.addr.clone()).await {
                    Ok(session) => {
                        let session = Arc::new(session);
                        let outcome = self.run_session(&conn, &session, &mut backoff).await;
                        conn.set_session(None);
                        session.close();
                        outcome
                    }
                    Err(e) => Err(e),
                };
            let why = match outcome {
                Ok(()) => "the event stream ended".to_owned(),
                Err(e) => e.to_string(),
            };
            tracing::info!("server {} offline: {why}", conn.addr);
            self.set_status(&conn, false, Some(why));
            tokio::time::sleep(backoff.next()).await;
        }
    }

    async fn run_session(
        &self,
        conn: &Arc<ServerConn>,
        session: &Arc<Session>,
        backoff: &mut Backoff,
    ) -> Result<(), ClientError> {
        let hello = Request::Hello {
            protocol: PROTOCOL_VERSION,
            name: self.name(),
        };
        match session.call(&hello).await? {
            Response::Hello { member } if member == self.0.member => {}
            Response::Hello { .. } => {
                return Err(ClientError::Net("server saw another key".into()));
            }
            other => return Err(crate::unexpected(&other)),
        }
        lock(&conn.joined).clear();
        let gone = self.join_pending(conn, session).await?;
        conn.set_session(Some(Arc::clone(session)));
        self.set_status(conn, true, None);
        self.report_gone(conn, gone);
        // A key added while the session was not yet published.
        let late = self.join_pending(conn, session).await?;
        self.report_gone(conn, late);

        let (mut out, reply) = within_stall(session.events.open(TransferMeta::default())).await?;
        within_stall(out.write_all(&to_bytes(&Subscribe {}))).await?;
        out.finish()?;
        let mut stream = within_stall(reply.recv()).await?;
        let mut seen = HashSet::new();
        let mut ready = false;
        loop {
            let frame = read_frame::<Event, _>(&mut stream, MAX_EVENT_FRAME);
            let frame = if ready {
                frame.await
            } else {
                // A server that stops sending before `Ready` has stalled.
                tokio::time::timeout(BACKLOG_STALL, frame)
                    .await
                    .map_err(|_| ClientError::Timeout)?
            };
            let Some(event) = frame? else {
                return Ok(());
            };
            match event {
                Event::Ready => {
                    ready = true;
                    backoff.reset();
                    self.drop_unseen(&conn.addr, &seen);
                }
                event => self.apply(&conn.addr, event, &mut seen),
            }
        }
    }

    /// Joins the kept rooms of the server that this session has not joined yet. Returns
    /// what to tell the user about rooms the server does not know.
    async fn join_pending(
        &self,
        conn: &ServerConn,
        session: &Session,
    ) -> Result<Vec<String>, ClientError> {
        let todo: Vec<RoomKey> = lock(&self.0.keys)
            .iter()
            .filter(|key| key.server == conn.addr && lock(&conn.joined).insert(key.token))
            .cloned()
            .collect();
        let mut gone = Vec::new();
        for key in todo {
            match session.call(&Request::JoinRoom { token: key.token }).await {
                Ok(_) => {}
                Err(ClientError::Api(ApiError::NoSuchRoom)) => {
                    gone.push(self.gone_message(RoomId::of(&key.token)));
                }
                Err(e) => {
                    lock(&conn.joined).remove(&key.token);
                    return Err(e);
                }
            }
        }
        Ok(gone)
    }

    /// Joins a room added while the server was already online.
    pub(crate) async fn join_late(&self, conn: &ServerConn) {
        let Some(session) = conn.session_now() else {
            return;
        };
        match self.join_pending(conn, &session).await {
            Ok(gone) => self.report_gone(conn, gone),
            Err(e) => tracing::warn!("joining a room on {} failed: {e}", conn.addr),
        }
    }

    fn report_gone(&self, conn: &ServerConn, messages: Vec<String>) {
        for error in messages {
            self.emit(ClientEvent::Status {
                server: conn.addr.clone(),
                online: true,
                error: Some(error),
            });
        }
    }

    fn gone_message(&self, room: RoomId) -> String {
        let label = lock(&self.0.rooms)
            .get(&room)
            .map_or_else(|| room.short(), |e| e.state.name.clone());
        format!("room {label} no longer exists")
    }

    /// Applies one event of `server`'s stream to the room states and tells the application.
    fn apply(&self, server: &ServerAddr, event: Event, seen: &mut HashSet<RoomId>) {
        let changed = match event {
            Event::Ready => return,
            Event::Left { room } => {
                self.forget_room(room);
                return;
            }
            Event::Release { version } => {
                self.emit(ClientEvent::Release {
                    server: server.clone(),
                    version,
                });
                return;
            }
            Event::Room(state) => {
                seen.insert(state.id);
                let room = state.id;
                lock(&self.0.rooms).insert(
                    room,
                    Entry {
                        server: server.clone(),
                        state,
                    },
                );
                room
            }
            Event::Task { room, task } => {
                if !self.update(room, |state| upsert_task(state, task)) {
                    return;
                }
                room
            }
            Event::Member { room, member } => {
                if !self.update(room, |state| upsert_member(state, member)) {
                    return;
                }
                room
            }
            Event::Sessions {
                room,
                member,
                sessions,
            } => {
                let found = self.update(room, |state| {
                    state.sessions.retain(|(id, _)| *id != member);
                    if !sessions.is_empty() {
                        state.sessions.push((member, sessions));
                    }
                });
                if !found {
                    return;
                }
                room
            }
            Event::Project { room, project } => {
                if !self.update(room, |state| upsert_project(state, project)) {
                    return;
                }
                room
            }
            Event::ProjectDeleted { room, project } => {
                if !self.update(room, |state| state.projects.retain(|p| p.id != project)) {
                    return;
                }
                room
            }
        };
        let state = lock(&self.0.rooms)
            .get(&changed)
            .map(|entry| entry.state.clone());
        if let Some(state) = state {
            self.emit(ClientEvent::Room(Box::new(state)));
        }
    }

    /// Runs `change` on the state of `room`; false when the room is not known.
    fn update(&self, room: RoomId, change: impl FnOnce(&mut RoomState)) -> bool {
        match lock(&self.0.rooms).get_mut(&room) {
            Some(entry) => {
                change(&mut entry.state);
                true
            }
            None => false,
        }
    }

    /// Removes the rooms of `server` that its backlog did not contain: the user is no longer
    /// in them.
    fn drop_unseen(&self, server: &ServerAddr, seen: &HashSet<RoomId>) {
        let stale: Vec<RoomId> = {
            let mut rooms = lock(&self.0.rooms);
            let stale = rooms
                .iter()
                .filter(|(id, entry)| entry.server == *server && !seen.contains(id))
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            for room in &stale {
                rooms.remove(room);
            }
            stale
        };
        for room in stale {
            self.emit(ClientEvent::RoomRemoved(room));
        }
    }
}

fn upsert_task(state: &mut RoomState, task: schwitzer_proto::Task) {
    match state.tasks.binary_search_by_key(&task.number, |t| t.number) {
        Ok(i) => state.tasks[i] = task,
        Err(i) => state.tasks.insert(i, task),
    }
}

/// Keeps the projects ordered by id.
fn upsert_project(state: &mut RoomState, project: schwitzer_proto::Project) {
    match state.projects.binary_search_by_key(&project.id, |p| p.id) {
        Ok(i) => state.projects[i] = project,
        Err(i) => state.projects.insert(i, project),
    }
}

fn upsert_member(state: &mut RoomState, member: Member) {
    match state.members.iter_mut().find(|m| m.id == member.id) {
        Some(existing) => *existing = member,
        None => state.members.push(member),
    }
}
