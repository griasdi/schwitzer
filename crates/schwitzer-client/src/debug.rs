//! The debug session of a room: starting, joining and leaving it, the log and instruction
//! traffic, the inbox of received instructions, and keeping both in step with the room state.

use schwitzer_proto::{
    ApiError, DebugId, DebugInstruction, DebugSession, MemberId, Request, Response, RoomId,
    ServerAddr,
};

use crate::{Client, ClientError, ClientEvent, lock};

/// Debug instructions kept per room; the oldest go first.
const INBOX_CAP: usize = 100;

/// A debug instruction this client received: as an addressee, or as the master who sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEntry {
    pub instruction: DebugInstruction,
    /// This member is an addressee.
    pub for_me: bool,
    /// An omp session took it ([`Client::take_debug_instruction`]).
    pub taken: bool,
}

impl Client {
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
            other => Err(ClientError::unexpected(&other)),
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
            other => Err(ClientError::unexpected(&other)),
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
        self.emit(ClientEvent::DebugInbox { room, fresh: None });
        Some(taken)
    }

    /// The id of `room`'s running debug session, if any.
    fn current_debug(&self, room: RoomId) -> Option<DebugId> {
        lock(&self.0.rooms)
            .get(&room)
            .and_then(|entry| entry.state.debug.as_ref().map(|s| s.id))
    }

    /// The id of `room`'s running debug session.
    fn debug_id(&self, room: RoomId) -> Result<DebugId, ClientError> {
        self.current_debug(room)
            .ok_or(ClientError::Api(ApiError::NoDebugSession))
    }

    /// Drops the debug inbox and the rejoin wish of a room that is gone from this machine.
    pub(crate) fn forget_debug(&self, room: RoomId) {
        lock(&self.0.debug_inbox).remove(&room);
        lock(&self.0.debug_rejoin).remove(&room);
    }

    /// Puts an instruction of `session` into the inbox of `room`, which starts afresh for a
    /// session it did not hold so far, and tells the application.
    pub(crate) fn receive_instruction(
        &self,
        room: RoomId,
        session: DebugId,
        instruction: DebugInstruction,
    ) {
        let entry = InboxEntry {
            for_me: instruction.to.contains(&self.member()),
            taken: false,
            instruction,
        };
        {
            let mut inboxes = lock(&self.0.debug_inbox);
            let inbox = inboxes.entry(room).or_insert((session, Vec::new()));
            if inbox.0 != session {
                *inbox = (session, Vec::new());
            }
            inbox.1.push(entry.clone());
            if inbox.1.len() > INBOX_CAP {
                let excess = inbox.1.len() - INBOX_CAP;
                inbox.1.drain(..excess);
            }
        }
        self.emit(ClientEvent::DebugInbox {
            room,
            fresh: Some(entry),
        });
    }

    /// Drops the inbox and the rejoin wish of `room` when they belong to a session that is
    /// not the room's current one.
    pub(crate) fn sync_debug(&self, room: RoomId) {
        let current = self.current_debug(room);
        let dropped = {
            let mut inboxes = lock(&self.0.debug_inbox);
            match inboxes.get(&room) {
                Some((id, entries)) if Some(*id) != current => {
                    let had = !entries.is_empty();
                    inboxes.remove(&room);
                    had
                }
                _ => false,
            }
        };
        {
            let mut rejoin = lock(&self.0.debug_rejoin);
            if rejoin.get(&room).is_some_and(|id| Some(*id) != current) {
                rejoin.remove(&room);
            }
        }
        if dropped {
            self.emit(ClientEvent::DebugInbox { room, fresh: None });
        }
    }

    /// Joins again the debug sessions of `server`'s rooms that this member had joined and
    /// lost through a disconnect: their human already agreed to them.
    pub(crate) fn rejoin_debug(&self, server: &ServerAddr) {
        let me = self.member();
        let pending: Vec<(RoomId, DebugId)> = {
            let rejoin = lock(&self.0.debug_rejoin);
            lock(&self.0.rooms)
                .iter()
                .filter(|(_, entry)| entry.server == *server)
                .filter_map(|(room, entry)| {
                    let session = entry.state.debug.as_ref()?;
                    (rejoin.get(room) == Some(&session.id) && !session.members.contains(&me))
                        .then_some((*room, session.id))
                })
                .collect()
        };
        for (room, session) in pending {
            let client = self.clone();
            self.0.handle.spawn(async move {
                if let Err(e) = client
                    .request(room, Request::DebugJoin { room, session })
                    .await
                {
                    tracing::warn!("rejoining the debug session failed: {e}");
                }
            });
        }
    }
}
