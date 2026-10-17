//! Requests on `/rpc`: one postcard `Request` in, one `Reply` out.

use serde::{Deserialize, Serialize};

use crate::{Decision, MemberId, Priority, RoomId, RoomToken, SessionInfo, Stage, Task, Work};

// Postcard is positional: enum variants are append-only.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    Hello {
        protocol: u32,
        name: String,
    },
    SetName {
        name: String,
    },
    CreateRoom {
        name: String,
    },
    JoinRoom {
        token: RoomToken,
    },
    LeaveRoom {
        room: RoomId,
    },
    Propose {
        room: RoomId,
        title: String,
        body: String,
    },
    EditTask {
        room: RoomId,
        task: u64,
        title: String,
        body: String,
        plan: String,
    },
    SetPriority {
        room: RoomId,
        task: u64,
        priority: Priority,
    },
    Decide {
        room: RoomId,
        task: u64,
        decision: Decision,
    },
    Claim {
        room: RoomId,
        work: Work,
        task: Option<u64>,
        agent: String,
    },
    SubmitPlan {
        room: RoomId,
        task: u64,
        plan: String,
    },
    Finish {
        room: RoomId,
        task: u64,
        note: String,
    },
    Release {
        room: RoomId,
        task: u64,
        note: String,
    },
    AddNote {
        room: RoomId,
        task: u64,
        text: String,
        agent: Option<String>,
    },
    PublishSessions {
        room: RoomId,
        sessions: Vec<SessionInfo>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Hello { member: MemberId },
    Ok,
    Room { id: RoomId, token: RoomToken },
    Task(Task),
    Claimed(Option<Task>),
}

/// Shown verbatim by the UI and over MCP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ApiError {
    #[error("the server speaks a different protocol version")]
    Protocol,
    #[error("the connection carries no client key")]
    NotAuthenticated,
    #[error("you are not a member of this room")]
    NotMember,
    #[error("no such room")]
    NoSuchRoom,
    #[error("no such task")]
    NoSuchTask,
    #[error("task is in stage {0:?}")]
    WrongStage(Stage),
    #[error("task is already claimed")]
    Claimed,
    #[error("the claim on this task is not yours")]
    NotYourClaim,
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("internal server error")]
    Internal,
}

pub type Reply = Result<Response, ApiError>;
