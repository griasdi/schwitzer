//! Rooms, tasks and members.

use serde::{Deserialize, Serialize};

use crate::{ClaimToken, MemberId, RoomId};

// Postcard is positional: enum variants are append-only.

/// Where a task stands. "In progress" is `Ready` with a Build claim;
/// "being planned" is `Accepted` with a Plan claim. `Idea` is a rough sketch that someone
/// writes up into a proposal; it comes first in the flow but is appended last for postcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    Proposed,
    Accepted,
    Planned,
    Ready,
    Done,
    Rejected,
    Idea,
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
    /// A human hands in the plan: `Accepted` with a non-empty plan becomes `Planned`.
    MarkPlanned,
    /// A human closes the build: `Ready` becomes `Done`.
    MarkDone,
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

/// A part of a room, for example one product or repository; every task belongs to one.
/// Ids are per-room numbers starting at 1, like task numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub number: u64,
    pub project: u64,
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
    /// Numbers of the tasks of the room this one waits for, ascending and without duplicates:
    /// a build claim skips it until all of them are `Done`.
    pub waits_for: Vec<u64>,
}

impl Task {
    /// The numbers in `waits_for` whose task in `tasks` is not `Done`, ascending. A number
    /// missing from `tasks` counts as not done.
    pub fn open_waits(&self, tasks: &[Task]) -> Vec<u64> {
        self.waits_for
            .iter()
            .copied()
            .filter(|n| {
                !tasks
                    .iter()
                    .any(|t| t.number == *n && t.stage == Stage::Done)
            })
            .collect()
    }
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
    /// The room's projects the session belongs to, ascending: the project of its linked task,
    /// else every project with a linked repo that holds its working directory.
    pub projects: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomState {
    pub id: RoomId,
    pub name: String,
    /// Ordered by id.
    pub projects: Vec<Project>,
    pub members: Vec<Member>,
    pub tasks: Vec<Task>,
    pub sessions: Vec<(MemberId, Vec<SessionInfo>)>,
}
