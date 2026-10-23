//! The `/events` stream: one `Subscribe` in, then `Event` frames out.

use serde::{Deserialize, Serialize};

use crate::{
    DebugId, DebugInstruction, DebugSession, Member, MemberId, Project, RoomId, RoomState,
    SessionInfo, Task,
};

/// Opens the stream. The server knows the member's rooms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscribe {}

// Postcard is positional: enum variants are append-only.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// Full state of one room: backlog on subscribe, and after create/join.
    Room(RoomState),
    /// The backlog is complete.
    Ready,
    Task {
        room: RoomId,
        task: Task,
    },
    Member {
        room: RoomId,
        member: Member,
    },
    Sessions {
        room: RoomId,
        member: MemberId,
        sessions: Vec<SessionInfo>,
    },
    Left {
        room: RoomId,
    },
    /// A new or renamed project.
    Project {
        room: RoomId,
        project: Project,
    },
    /// The newest stable app release on GitHub. Sent after `Ready` when the server knows one,
    /// and to every stream whenever it changes.
    Release {
        version: String,
    },
    /// A deleted project; it had no tasks.
    ProjectDeleted {
        room: RoomId,
        project: u64,
    },
    /// The room's debug session changed; `None`: it ended. Sent to every member of the room.
    Debug {
        room: RoomId,
        session: Option<DebugSession>,
    },
    /// Log bytes of `from`'s file `stream`, starting at byte `offset` of that file. Sent to
    /// the other participants.
    DebugLog {
        room: RoomId,
        session: DebugId,
        from: MemberId,
        stream: String,
        offset: u64,
        data: Vec<u8>,
    },
    /// Sent to the addressees and the master.
    DebugInstruction {
        room: RoomId,
        session: DebugId,
        instruction: DebugInstruction,
    },
}
