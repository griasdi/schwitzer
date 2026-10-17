//! Rooms, tasks and members.

use serde::{Deserialize, Serialize};

use crate::{ClaimToken, MemberId, RoomId};

// Postcard is positional: enum variants are append-only.

/// Where a task stands. "In progress" is `Ready` with a Build claim;
/// "being planned" is `Accepted` with a Plan claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    Proposed,
    Accepted,
    Planned,
    Ready,
    Done,
    Rejected,
}

/// Sort order: `High` first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Work {
    Plan,
    Build,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Decision {
    Accept,
    Reject,
    Approve,
    Replan,
    Reopen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub member: MemberId,
    pub agent: String,
    pub work: Work,
    pub since_ms: u64,
    pub token: ClaimToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub author: MemberId,
    pub agent: Option<String>,
    pub text: String,
    pub at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub plan: String,
    pub stage: Stage,
    pub priority: Priority,
    pub author: MemberId,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub claim: Option<Claim>,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: MemberId,
    pub name: String,
    pub online: bool,
}

/// One live agent session of a member, linked to a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub project: String,
    pub model: String,
    pub started_ms: u64,
    pub active_ms: u64,
    pub tokens: u64,
    pub task: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomState {
    pub id: RoomId,
    pub name: String,
    pub members: Vec<Member>,
    pub tasks: Vec<Task>,
    pub sessions: Vec<(MemberId, Vec<SessionInfo>)>,
}
