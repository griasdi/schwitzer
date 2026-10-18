//! The MCP server: lets agents on this computer list, propose, claim and finish tasks of the
//! joined rooms as this member.
//!
//! Streamable HTTP (rmcp 3.5) on `127.0.0.1` only. Every request needs the bearer token from
//! `<data dir>/mcp-token`; requests with an `Origin` header (browsers) are refused, and rmcp's
//! loopback Host allowlist stays on. None of this is configurable. Agents cannot accept,
//! reject, approve, replan or reopen: those decisions stay with humans in the UI.

mod serve;
mod token;
mod tools;

pub use serve::{client_config, subscription, url};
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
