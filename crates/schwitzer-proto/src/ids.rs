//! Identifiers. Every id is a fixed-size byte array displayed as lowercase hex.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Error of the hex `FromStr` implementations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid id: expected {expected} hex characters")]
pub struct IdParseError {
    pub expected: usize,
}

macro_rules! byte_id {
    ($(#[$doc:meta])* $name:ident, $len:expr) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub [u8; $len]);

        impl $name {
            pub const LEN: usize = $len;

            pub fn from_slice(bytes: &[u8]) -> Option<Self> {
                <[u8; $len]>::try_from(bytes).ok().map(Self)
            }

            pub fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                hex::encode(self.0)
            }

            /// The first eight hex digits: enough to tell ids apart on screen.
            pub fn short(&self) -> String {
                self.to_hex()[..8].to_owned()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&hex::encode(self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), hex::encode(self.0))
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let mut out = [0u8; $len];
                hex::decode_to_slice(s.trim(), &mut out)
                    .map_err(|_| IdParseError { expected: $len * 2 })?;
                Ok(Self(out))
            }
        }
    };
}

macro_rules! random_id {
    ($name:ident) => {
        impl $name {
            /// A fresh random id.
            pub fn random() -> Self {
                Self(rand::random())
            }
        }
    };
}

byte_id!(
    /// SHA-256 SPKI fingerprint of the app's weida key.
    MemberId,
    32
);
byte_id!(
    /// The room secret; whoever holds it can join.
    RoomToken,
    32
);
byte_id!(
    /// `blake3(token)`: names a room without revealing its secret.
    RoomId,
    32
);
byte_id!(
    /// Marks one claim; agents echo it into their session log.
    ClaimToken,
    16
);
byte_id!(
    /// A debug session; random, never reused.
    DebugId,
    16
);

random_id!(RoomToken);
random_id!(ClaimToken);
random_id!(DebugId);

impl RoomId {
    pub fn of(token: &RoomToken) -> Self {
        Self(*blake3::hash(&token.0).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_and_length_check() {
        let id = ClaimToken::random();
        assert_eq!(id.to_string().parse::<ClaimToken>().unwrap(), id);
        assert!("abcd".parse::<ClaimToken>().is_err());
    }
}
