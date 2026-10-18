//! The `/events` stream: one `Subscribe` in, then `Event` frames out.

use serde::{Deserialize, Serialize};

use crate::{Member, MemberId, Project, RoomId, RoomState, SessionInfo, Task};

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
    /// The newest stable app release whose version is not above the server's own. Sent after
    /// `Ready` when the server knows one, and to every stream whenever it changes.
    Release {
        version: String,
    },
}
