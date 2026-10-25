//! Server addresses and room keys as strings.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::RoomToken;

/// How to reach a schwitzer server: host, UDP port and the SHA-256
/// fingerprint of its certificate key.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ServerAddr {
    pub host: String,
    pub port: u16,
    pub fingerprint: [u8; 32],
}

impl ServerAddr {
    /// `weida://sha256:<hex>@host:port<path>`.
    pub fn url(&self, path: &str) -> String {
        format!(
            "weida://sha256:{}@{}{}",
            hex::encode(self.fingerprint),
            self.authority(),
            path
        )
    }

    /// `host:port`, an IPv6 host in brackets.
    pub fn authority(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

impl fmt::Display for ServerAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url(""))
    }
}

impl fmt::Debug for ServerAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ServerAddr({} sha256:{})",
            self.authority(),
            hex::encode(&self.fingerprint[..8])
        )
    }
}

/// Which part of an address or key failed to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AddrParseError {
    #[error("bad scheme: expected {0}")]
    Scheme(&'static str),
    #[error("bad fingerprint: expected sha256:<64 hex characters>")]
    Fingerprint,
    #[error("bad host")]
    Host,
    #[error("bad port")]
    Port,
    #[error("bad token: expected 64 hex characters")]
    Token,
}

const SERVER_SCHEME: &str = "weida://";
const KEY_SCHEME: &str = "schwitzer://";

/// Parses `sha256:<hex>@host:port` (the part after the scheme).
fn parse_server(rest: &str) -> Result<ServerAddr, AddrParseError> {
    let (fp, authority) = rest.split_once('@').ok_or(AddrParseError::Fingerprint)?;
    let fp = fp
        .strip_prefix("sha256:")
        .ok_or(AddrParseError::Fingerprint)?;
    let mut fingerprint = [0u8; 32];
    hex::decode_to_slice(fp, &mut fingerprint).map_err(|_| AddrParseError::Fingerprint)?;
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (host, rest) = v6.split_once(']').ok_or(AddrParseError::Host)?;
        let port = rest.strip_prefix(':').ok_or(AddrParseError::Port)?;
        (host, port)
    } else {
        authority.rsplit_once(':').ok_or(AddrParseError::Port)?
    };
    if host.is_empty()
        || host
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '@' | '[' | ']'))
        || (!authority.starts_with('[') && host.contains(':'))
    {
        return Err(AddrParseError::Host);
    }
    let port = port.parse::<u16>().map_err(|_| AddrParseError::Port)?;
    Ok(ServerAddr {
        host: host.to_owned(),
        port,
        fingerprint,
    })
}

impl FromStr for ServerAddr {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let rest = s
            .strip_prefix(SERVER_SCHEME)
            .ok_or(AddrParseError::Scheme(SERVER_SCHEME))?;
        parse_server(rest.strip_suffix('/').unwrap_or(rest))
    }
}

/// What a user copies and shares: the server plus the room secret.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomKey {
    pub server: ServerAddr,
    pub token: RoomToken,
}

impl fmt::Display for RoomKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{KEY_SCHEME}sha256:{}@{}/{}",
            hex::encode(self.server.fingerprint),
            self.server.authority(),
            self.token
        )
    }
}

impl fmt::Debug for RoomKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RoomKey({:?})", self.server)
    }
}

impl FromStr for RoomKey {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let rest = s
            .strip_prefix(KEY_SCHEME)
            .ok_or(AddrParseError::Scheme(KEY_SCHEME))?;
        let (server, token) = rest.rsplit_once('/').ok_or(AddrParseError::Token)?;
        let server = parse_server(server)?;
        let token = token
            .parse::<RoomToken>()
            .map_err(|_| AddrParseError::Token)?;
        Ok(RoomKey { server, token })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(host: &str) -> ServerAddr {
        ServerAddr {
            host: host.into(),
            port: 7444,
            fingerprint: [0xab; 32],
        }
    }

    #[test]
    fn server_and_key_roundtrip() {
        for host in ["example.org", "10.0.0.7", "::1"] {
            let a = addr(host);
            let s = a.to_string();
            assert_eq!(s.parse::<ServerAddr>().unwrap(), a, "{s}");
            assert_eq!(format!("{s}/").parse::<ServerAddr>().unwrap(), a);
            let key = RoomKey {
                server: a,
                token: RoomToken([0x5c; 32]),
            };
            let k = key.to_string();
            assert!(k.starts_with("schwitzer://sha256:abab"));
            assert_eq!(format!("  {k}\n").parse::<RoomKey>().unwrap(), key, "{k}");
        }
        assert!(addr("::1").to_string().ends_with("@[::1]:7444"));
        assert!(addr("::1").url("/rpc").ends_with("@[::1]:7444/rpc"));
    }

    #[test]
    fn each_bad_key_part_has_its_own_error() {
        let fp = "ab".repeat(32);
        let tok = "5c".repeat(32);
        let cases = [
            (
                format!("weida://sha256:{fp}@h:1/{tok}"),
                AddrParseError::Scheme(KEY_SCHEME),
            ),
            (
                format!("schwitzer://sha256:{}@h:1/{tok}", &fp[2..]),
                AddrParseError::Fingerprint,
            ),
            (
                format!("schwitzer://md5:{fp}@h:1/{tok}"),
                AddrParseError::Fingerprint,
            ),
            (
                format!("schwitzer://sha256:{fp}@:1/{tok}"),
                AddrParseError::Host,
            ),
            (
                format!("schwitzer://sha256:{fp}@h:99999/{tok}"),
                AddrParseError::Port,
            ),
            (
                format!("schwitzer://sha256:{fp}@h/{tok}"),
                AddrParseError::Port,
            ),
            (
                format!("schwitzer://sha256:{fp}@h:1/zz"),
                AddrParseError::Token,
            ),
        ];
        for (input, want) in cases {
            assert_eq!(input.parse::<RoomKey>(), Err(want), "{input}");
        }
    }
}
