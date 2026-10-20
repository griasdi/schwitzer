//! Requests on `/rpc`: one postcard `Request` in, one `Reply` out.

use serde::{Deserialize, Serialize};

use crate::{
    Decision, MemberId, Priority, Project, RoomId, RoomToken, SessionInfo, Stage, Task, Work,
};

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
        project: u64,
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
        /// Restricts only the automatic choice (`task: None`); ignored when `task` is given.
        project: Option<u64>,
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
    Unclaim {
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
    /// A new task in stage `Idea`.
    AddIdea {
        room: RoomId,
        project: u64,
        title: String,
        body: String,
    },
    /// Writes an `Idea` up into a proposal: new title and body, stage `Proposed`, same number.
    ProposeIdea {
        room: RoomId,
        task: u64,
        title: String,
        body: String,
    },
    /// A new project; its name is unique in the room, ignoring case.
    CreateProject {
        room: RoomId,
        name: String,
    },
    /// Renames a project; the new name is unique in the room, ignoring case.
    RenameProject {
        room: RoomId,
        project: u64,
        name: String,
    },
    /// Moves a task, in any stage, into another project of its room.
    MoveTask {
        room: RoomId,
        task: u64,
        project: u64,
    },
    /// Deletes a project without tasks in any stage; its id is never given again.
    DeleteProject {
        room: RoomId,
        project: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Hello { member: MemberId },
    Ok,
    Room { id: RoomId, token: RoomToken },
    Task(Task),
    Claimed(Option<Task>),
    Project(Project),
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
    #[error("no such project")]
    NoSuchProject,
    #[error("a project with this name already exists")]
    ProjectExists,
    #[error("the project still has tasks; move them into another project first")]
    ProjectNotEmpty,
}

pub type Reply = Result<Response, ApiError>;
