//! The MCP server: the omp plugin in shared repos lists, proposes, claims and finishes tasks of
//! the joined rooms through it, as this member.
//!
//! Streamable HTTP (rmcp 3.5) on `127.0.0.1` only. Every request needs the bearer token from
//! `<data dir>/mcp-token`; requests with an `Origin` header (browsers) are refused, and rmcp's
//! loopback Host allowlist stays on. None of this is configurable. Agents cannot accept,
//! reject, approve, replan or reopen: decisions come only through the tools whose description
//! starts "For the omp plugin, not for agents.", which the plugin never hands to its agent and
//! calls only from the human's own dialogs. Approving a plan is always allowed there; every
//! other decision needs the fast lane of the task or of its project ([`FastLane`]).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use schwitzer_proto::RoomId;

use crate::settings::Lanes;

mod serve;
mod token;
mod tools;

pub use serve::{subscription, url};
pub use token::load_or_create_token;

/// Whether the server listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpStatus {
    /// The server accepts connections on `url`.
    Listening { url: String },
    /// Another program holds the port; the server keeps trying and comes up when it is free.
    PortInUse,
    /// The listener could not start or stopped; holds the reason.
    Failed(String),
}

impl McpStatus {
    /// Why the server is down, for the pages that spell it out; `None` while it listens.
    /// `port` is the port the settings ask for.
    pub fn problem(&self, port: u16) -> Option<String> {
        match self {
            Self::Listening { .. } => None,
            Self::PortInUse => Some(format!("Port {port} is in use")),
            Self::Failed(reason) => Some(reason.clone()),
        }
    }
}

/// What the MCP server tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpEvent {
    Status(McpStatus),
    /// The omp plugin put a task into the fast lane or took it out.
    FastLaneTask {
        room: RoomId,
        task: u64,
        on: bool,
    },
}

/// The app's copy of this member's [`Lanes`] as the MCP tools see them. The app applies every
/// change of the lanes to its settings and to this copy alike ([`FastLane::change`]) and never
/// copies the whole settings in after the start; `set_fast_lane` writes a task flag here at
/// once and sends [`McpEvent::FastLaneTask`], which the app applies again.
#[derive(Clone, Default)]
pub struct FastLane(Arc<Mutex<Lanes>>);

impl FastLane {
    /// A copy holding `lanes`.
    pub fn new(lanes: Lanes) -> Self {
        Self(Arc::new(Mutex::new(lanes)))
    }

    fn lanes(&self) -> MutexGuard<'_, Lanes> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Applies `change` to the lanes and returns what it returns.
    pub fn change<R>(&self, change: impl FnOnce(&mut Lanes) -> R) -> R {
        change(&mut self.lanes())
    }

    /// Whether the fast lane of `project` in `room` is on.
    pub fn project(&self, room: &RoomId, project: u64) -> bool {
        self.lanes().project(room, project)
    }

    /// Whether task `task` of `room` is in the fast lane.
    pub fn task(&self, room: &RoomId, task: u64) -> bool {
        self.lanes().task(room, task)
    }

    /// Whether task `number` of `room` is in a lane, see [`Lanes::covers`].
    pub fn covers(&self, room: &RoomId, number: u64, project: Option<u64>) -> bool {
        self.lanes().covers(room, number, project)
    }
}
