//! The access token of the MCP server: a random 64-digit hex string in `mcp-token` of the
//! data directory, readable by the user only on unix.

use std::io::Write;
use std::path::Path;

/// File in the data directory that holds the access token.
pub const TOKEN_FILE: &str = "mcp-token";

/// The stored access token, or a new one when none (or a malformed one) is stored, or when the
/// file lets group or others read it (on unix): that token has to be assumed read, so it is
/// replaced, not kept.
pub fn load_or_create_token(data: &Path) -> std::io::Result<String> {
    let path = data.join(TOKEN_FILE);
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Some(token) = stored_token(&text)
        && private(&path)
    {
        return Ok(token.to_owned());
    }
    create_token(data)
}

/// Whether only the owner has access to the file at `path`; always where there are no unix
/// permissions.
fn private(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| owner_only(m.permissions().mode()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

/// Whether the unix file `mode` gives nothing to group and others.
#[cfg(unix)]
fn owner_only(mode: u32) -> bool {
    mode & 0o077 == 0
}

/// The access token in the `text` of the token file: 64 lowercase hex digits, without the
/// whitespace around them.
fn stored_token(text: &str) -> Option<&str> {
    let token = text.trim();
    (token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(token)
}

/// A new access token: 32 bytes from the OS-seeded CSPRNG, hex.
fn new_token() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

/// Writes a new random access token (32 bytes from the OS-seeded CSPRNG, hex) and returns
/// it. Readable by the user only on unix.
fn create_token(data: &Path) -> std::io::Result<String> {
    std::fs::create_dir_all(data)?;
    let token = new_token();
    let tmp = data.join(format!("{TOKEN_FILE}.tmp"));
    // Created anew: a file that is already there (a leftover, or someone else's) would keep
    // its mode and its owner, and the token would be written into it.
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp)?;
    file.write_all(token.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, data.join(TOKEN_FILE))?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_token_is_64_lowercase_hex_digits() {
        let token = "0123456789abcdef".repeat(4);
        assert_eq!(stored_token(&token), Some(token.as_str()));
        // A hand-edited file ends with a newline.
        assert_eq!(stored_token(&format!("{token}\n")), Some(token.as_str()));
        assert_eq!(stored_token(&token[..63]), None);
        assert_eq!(stored_token(&format!("{token}0")), None);
        assert_eq!(stored_token(&token.to_uppercase()), None);
        assert_eq!(stored_token("not a token"), None);
        assert_eq!(stored_token(""), None);
    }

    #[test]
    fn new_tokens_are_valid_and_differ() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(stored_token(&a), Some(a.as_str()));
        assert_eq!(stored_token(&b), Some(b.as_str()));
        assert_ne!(a, b);
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_may_have_access_to_a_token_file() {
        for mode in [0o600, 0o400, 0o700] {
            assert!(owner_only(mode), "{mode:o}");
        }
        for mode in [0o640, 0o604, 0o644, 0o660, 0o666, 0o601] {
            assert!(!owner_only(mode), "{mode:o}");
        }
    }
}
