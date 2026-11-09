//! Requests on `/rpc`: one postcard `Request` in, one `Reply` out.

use serde::{Deserialize, Serialize};

use crate::{
    DebugId, DebugInstruction, DebugSession, Decision, MemberId, Part, Priority, Project, Revision,
    RoomId, RoomToken, SessionInfo, Stage, Suggestion, Task, Work,
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
        waits_for: Vec<u64>,
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
    /// Writes an `Idea` up into a proposal: new title and body, stage `Proposed`, same number;
    /// `waits_for` is added to what the idea already waits for.
    ProposeIdea {
        room: RoomId,
        task: u64,
        title: String,
        body: String,
        waits_for: Vec<u64>,
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
    /// Replaces the tasks a task waits for; not allowed once it is `Done` or `Rejected`.
    SetWaits {
        room: RoomId,
        task: u64,
        waits_for: Vec<u64>,
    },
    /// Starts a debug session in the room with the caller as master.
    DebugStart {
        room: RoomId,
    },
    DebugJoin {
        room: RoomId,
        session: DebugId,
    },
    /// A participant leaves; the master leaving ends the session for everyone.
    DebugLeave {
        room: RoomId,
        session: DebugId,
    },
    /// Bytes of the caller's file `stream`, starting at byte `offset` of that file.
    DebugLog {
        room: RoomId,
        session: DebugId,
        stream: String,
        offset: u64,
        data: Vec<u8>,
    },
    /// `to` empty: every participant except the master.
    DebugInstruct {
        room: RoomId,
        session: DebugId,
        to: Vec<MemberId>,
        text: String,
    },
    /// An agent's whole new text for `part` (title only with `Details`), written on `version`;
    /// changes nothing until applied.
    SuggestRevision {
        room: RoomId,
        task: u64,
        part: Part,
        version: u64,
        title: String,
        text: String,
        note: String,
        agent: Option<String>,
    },
    /// Makes the suggestion the task's text (a new version) and drops it; not once the task is
    /// `Done` or `Rejected`.
    ApplySuggestion {
        room: RoomId,
        task: u64,
        suggestion: u64,
    },
    DiscardSuggestion {
        room: RoomId,
        task: u64,
        suggestion: u64,
    },
    /// The stored versions of `part`, newest first, as many as fit one reply.
    History {
        room: RoomId,
        task: u64,
        part: Part,
    },
    GetSuggestion {
        room: RoomId,
        task: u64,
        suggestion: u64,
    },
    /// A comment on the block `quote` of `part`, written on `version`; not once the task is
    /// `Done` or `Rejected`.
    Annotate {
        room: RoomId,
        task: u64,
        part: Part,
        version: u64,
        quote: String,
        text: String,
    },
    /// Marks a comment resolved, or open again; in every stage.
    ResolveAnnotation {
        room: RoomId,
        task: u64,
        annotation: u64,
        resolved: bool,
    },
    /// Upvotes the idea (`up`) or takes the caller's vote back; a repeat changes nothing.
    /// Only while the task is an `Idea`.
    Vote {
        room: RoomId,
        task: u64,
        up: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Hello {
        member: MemberId,
    },
    Ok,
    Room {
        id: RoomId,
        token: RoomToken,
    },
    Task(Task),
    Claimed(Option<Task>),
    Project(Project),
    Debug(DebugSession),
    DebugInstruction(DebugInstruction),
    History(Vec<Revision>),
    Suggestion {
        suggestion: Suggestion,
        title: String,
        text: String,
    },
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
    #[error("the task waits for #{0}, which is not done yet")]
    Waiting(u64),
    #[error("no debug session is running in this room")]
    NoDebugSession,
    #[error("a debug session is already running in this room")]
    DebugRunning,
    #[error("you have not joined the debug session")]
    NotInDebugSession,
    #[error("only the master of the debug session can do this")]
    NotDebugMaster,
    #[error("too much log data, try again in {0} s")]
    LogBudget(u64),
    #[error("no such suggestion; it was applied or discarded")]
    NoSuchSuggestion,
    #[error("no such comment")]
    NoSuchAnnotation,
}

pub type Reply = Result<Response, ApiError>;
