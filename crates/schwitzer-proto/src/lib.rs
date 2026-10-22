//! schwitzer wire types, ids, framing and keys.

pub mod addr;
pub mod events;
pub mod frame;
pub mod ids;
pub mod model;
pub mod rpc;

use std::time::Duration;

use serde::{Serialize, de::DeserializeOwned};

pub use addr::{AddrParseError, RoomKey, ServerAddr};
pub use events::{Event, Subscribe};
pub use frame::{FrameError, HEADER_LEN, encode_frame, read_frame};
pub use ids::{ClaimToken, IdParseError, MemberId, RoomId, RoomToken};
pub use model::{
    Claim, Decision, Member, Note, Priority, Project, RoomState, SessionInfo, Stage, Task, Work,
};
pub use rpc::{ApiError, Reply, Request, Response};

pub const PROTOCOL_VERSION: u32 = 7;

pub const PATH_RPC: &str = "/rpc";
pub const PATH_EVENTS: &str = "/events";

pub const MAX_RPC_BYTES: usize = 1 << 20;
pub const MAX_EVENT_FRAME: usize = 4 << 20;
pub const RPC_TIMEOUT: Duration = Duration::from_secs(15);

/// Field caps, in bytes.
pub mod caps {
    pub const ROOM_NAME: usize = 100;
    pub const PROJECT_NAME: usize = 100;
    pub const TITLE: usize = 200;
    pub const BODY: usize = 64 * 1024;
    pub const PLAN: usize = 256 * 1024;
    pub const NOTE: usize = 16 * 1024;
    pub const AGENT_LABEL: usize = 100;
    pub const MEMBER_NAME: usize = 64;
    pub const SESSIONS_PER_PUBLISH: usize = 50;
    pub const SESSION_TITLE: usize = 200;
    /// Projects one published session may name.
    pub const SESSION_PROJECTS: usize = 64;
    /// Tasks one task may wait for.
    pub const WAITS_FOR: usize = 32;

    /// `text` cut to at most `max` bytes at a character boundary.
    pub fn cut(text: &str, max: usize) -> &str {
        if text.len() <= max {
            return text;
        }
        let end = (0..=max)
            .rev()
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0);
        &text[..end]
    }

    #[cfg(test)]
    mod tests {
        use super::cut;

        #[test]
        fn text_is_cut_at_a_character_boundary() {
            assert_eq!(cut("abc", 5), "abc");
            assert_eq!(cut("äbc", 1), "");
            assert_eq!(cut("äbc", 2), "ä");
        }
    }
}

/// Error of the postcard helpers.
#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("decoding failed: {0}")]
    Decode(postcard::Error),
}

/// Serializes `value` with postcard. Serialization of the wire types into a
/// growable buffer cannot fail; a failure is a programming error.
pub fn to_bytes<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    postcard::to_stdvec(value).expect("postcard serialization into a Vec")
}

/// Length of the postcard encoding of `value`, computed without encoding it.
pub fn encoded_len<T: Serialize + ?Sized>(value: &T) -> usize {
    postcard::experimental::serialized_size(value).expect("postcard size of the wire types")
}

/// Deserializes a postcard value.
pub fn from_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtoError> {
    postcard::from_bytes(bytes).map_err(ProtoError::Decode)
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
