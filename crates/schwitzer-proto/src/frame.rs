//! Length-prefixed frames: `u32 LE length || postcard(value)`.

use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt};

/// What a frame body reserves before its bytes have arrived. A peer can
/// declare any length up to `max` and then stall, so the buffer grows with
/// what was really received instead.
const FIRST_RESERVE: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("frame too large: {0} bytes")]
    TooLarge(usize),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Decode(#[from] postcard::Error),
}

/// Encodes `value` as one frame.
pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, FrameError> {
    frame_bytes(&postcard::to_stdvec(value)?)
}

/// Puts the length prefix before `body`, which is already encoded (a sealed
/// chunk, say). A body above `u32::MAX` bytes is [`FrameError::TooLarge`].
pub fn frame_bytes(body: &[u8]) -> Result<Vec<u8>, FrameError> {
    let len = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge(body.len()))?;
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

/// Reads one frame. `Ok(None)` on a clean end of stream before a length
/// prefix; a length above `max` is [`FrameError::TooLarge`].
pub async fn read_frame<T, R>(reader: &mut R, max: usize) -> Result<Option<T>, FrameError>
where
    T: DeserializeOwned,
    R: AsyncRead + Unpin,
{
    match read_frame_body(reader, max).await? {
        Some(body) => Ok(Some(postcard::from_bytes(&body)?)),
        None => Ok(None),
    }
}

/// Reads one frame's body without decoding it, for a value that decodes
/// across wire versions. `Ok(None)` and the errors as in [`read_frame`].
pub async fn read_frame_body<R: AsyncRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> Result<Option<Vec<u8>>, FrameError> {
    let mut len = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        let n = reader.read(&mut len[filled..]).await?;
        if n == 0 {
            if filled == 0 {
                return Ok(None);
            }
            return Err(FrameError::Io(std::io::ErrorKind::UnexpectedEof.into()));
        }
        filled += n;
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > max {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = Vec::with_capacity(len.min(FIRST_RESERVE));
    read_body(reader, len, &mut body).await?;
    Ok(Some(body))
}

/// Appends exactly `len` bytes of `reader` to `body`; a shorter stream is
/// `UnexpectedEof`.
async fn read_body<R: AsyncRead + Unpin>(
    reader: &mut R,
    len: usize,
    body: &mut Vec<u8>,
) -> std::io::Result<()> {
    (&mut *reader).take(len as u64).read_to_end(body).await?;
    if body.len() != len {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;

    #[tokio::test]
    async fn roundtrip_eof_and_oversize() {
        let mut buf = encode_frame(&Event::Ready).unwrap();
        buf.extend(
            encode_frame(&Event::Left {
                room: crate::RoomId([7; 32]),
            })
            .unwrap(),
        );
        let mut r = &buf[..];
        assert_eq!(
            read_frame::<Event, _>(&mut r, 1024).await.unwrap(),
            Some(Event::Ready)
        );
        assert!(matches!(
            read_frame::<Event, _>(&mut r, 1024).await.unwrap(),
            Some(Event::Left { .. })
        ));
        assert_eq!(read_frame::<Event, _>(&mut r, 1024).await.unwrap(), None);

        let big = encode_frame(&vec![0u8; 100]).unwrap();
        let mut r = &big[..];
        assert!(matches!(
            read_frame::<Vec<u8>, _>(&mut r, 10).await,
            Err(FrameError::TooLarge(_))
        ));

        let truncated = &buf[..2];
        let mut r = truncated;
        assert!(read_frame::<Event, _>(&mut r, 1024).await.is_err());
    }

    #[tokio::test]
    async fn a_declared_length_does_not_reserve_memory() {
        let len = 1 << 20;
        let mut wire = (len as u32).to_le_bytes().to_vec();
        wire.extend_from_slice(&[7u8; 10]);
        let mut r = &wire[..];
        assert!(matches!(
            read_frame::<Vec<u8>, _>(&mut r, 2 << 20).await,
            Err(FrameError::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof
        ));

        let mut body = Vec::with_capacity(len.min(FIRST_RESERVE));
        let mut r = &wire[4..];
        assert!(read_body(&mut r, len, &mut body).await.is_err());
        assert_eq!(body.len(), 10);
        assert!(body.capacity() < len);
    }
}
