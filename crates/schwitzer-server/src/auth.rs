//! The caller of a request: the key its QUIC handshake proved.

use schwitzer_proto::{ApiError, MemberId};
use weida::{IncomingRequest, PeerIdentity};

/// The member behind `request`; a peer that proved no key is `NotAuthenticated`.
pub fn caller(request: &IncomingRequest) -> Result<MemberId, ApiError> {
    match &request.meta().peer {
        Some(PeerIdentity::Key(fingerprint)) => Ok(MemberId(*fingerprint.as_bytes())),
        _ => Err(ApiError::NotAuthenticated),
    }
}
