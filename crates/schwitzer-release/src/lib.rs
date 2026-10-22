//! What the app's self-update and the server's `--auto-update` share: the GitHub release list and
//! its downloads, the `sha256:` digest GitHub publishes, the bounded HTTPS requests, the size and
//! digest check of a download, the member of a `.tar.gz` and the swap of a program file.
//!
//! What differs stays with the callers: which asset to take, how to read a release, the archive
//! formats beyond `.tar.gz`, and how the new program starts.

#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// GitHub API URL of the schwitzer release list.
pub const RELEASES_URL: &str = "https://api.github.com/repos/griasdi/schwitzer/releases";

/// The release asset suffix of this build's platform; `None` where no release is built.
pub const PLATFORM: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-x86_64")
} else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
    Some("linux-aarch64")
} else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("macos-aarch64")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("windows-x86_64")
} else {
    None
};

/// A download of a GitHub release, as the release shows it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    pub digest: Option<String>,
}

/// An archive to download, and what it must be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub url: String,
    pub size: u64,
    pub sha256: [u8; 32],
}

impl Asset {
    /// The download this asset stands for; it needs the `sha256:` digest GitHub lists.
    pub fn download_info(&self) -> Result<Download, String> {
        let mut sha256 = [0; 32];
        self.digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .and_then(|h| hex::decode_to_slice(h, &mut sha256).ok())
            .ok_or_else(|| format!("{} has no sha256 digest", self.name))?;
        Ok(Download {
            url: self.browser_download_url.clone(),
            size: self.size,
            sha256,
        })
    }
}

/// The version a release tag names: `v1.2.3` or `1.2.3`.
pub fn parse_tag(tag: &str) -> Option<semver::Version> {
    tag.strip_prefix('v').unwrap_or(tag).parse().ok()
}

/// Whether `version` is a stable one: no pre-release part.
pub fn is_stable(version: &semver::Version) -> bool {
    version.pre.is_empty()
}

/// Why a request to the GitHub API failed; the message is the one to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// GitHub answered 404.
    NotFound(String),
    Failed(String),
}

impl From<ApiError> for String {
    fn from(e: ApiError) -> String {
        match e {
            ApiError::NotFound(message) | ApiError::Failed(message) => message,
        }
    }
}

fn agent(user_agent: &str, timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(user_agent)
        .build()
        .into()
}

/// GETs `url` from the GitHub API with `query` and reads at most `limit` bytes of the answer.
/// Blocks.
pub fn api_get(
    user_agent: &str,
    timeout: Duration,
    url: &str,
    query: &[(&str, &str)],
    limit: u64,
) -> Result<Vec<u8>, ApiError> {
    let mut request = agent(user_agent, timeout).get(url);
    for (key, value) in query {
        request = request.query(key, value);
    }
    let mut response = request
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| {
            let message = format!("GitHub: {e}");
            match e {
                ureq::Error::StatusCode(404) => ApiError::NotFound(message),
                _ => ApiError::Failed(message),
            }
        })?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| ApiError::Failed(format!("GitHub: {e}")))
}

/// Downloads `url`, at most `limit` bytes. Blocks.
pub fn download(
    user_agent: &str,
    timeout: Duration,
    url: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let mut response = agent(user_agent, timeout)
        .get(url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("download failed: {e}"))
}

/// Checks that `bytes` have the `size` and sha256 digest the release lists.
pub fn verify(bytes: &[u8], size: u64, sha256: &[u8; 32]) -> Result<(), String> {
    let got = bytes.len() as u64;
    if got != size {
        return Err(format!("download is {got} bytes, expected {size}"));
    }
    if Sha256::digest(bytes).as_slice() != sha256 {
        return Err("download does not match its sha256 digest".into());
    }
    Ok(())
}

/// The error text for an archive that cannot be read.
pub fn unreadable(e: impl std::fmt::Display) -> String {
    format!("unreadable archive: {e}")
}

/// The regular file named `name` at any depth of the `.tar.gz` `bytes`, at most `limit` bytes of
/// it.
#[cfg(unix)]
pub fn extract_tar_gz(bytes: &[u8], name: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries().map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let is_member = entry.header().entry_type().is_file()
            && entry.path().map_err(unreadable)?.file_name() == Some(OsStr::new(name));
        if is_member {
            let mut member = Vec::new();
            entry
                .take(limit)
                .read_to_end(&mut member)
                .map_err(unreadable)?;
            return Ok(member);
        }
    }
    Err(format!("the archive holds no {name}"))
}

/// `exe` with `suffix` appended to its file name.
pub fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut path = exe.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

/// Writes `binary` beside `exe` and swaps it in. A rename gives a new file, so the running
/// process keeps the old one, and macOS checks the new file's signature afresh.
#[cfg(unix)]
pub fn replace(exe: &Path, binary: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let new = sibling(exe, ".new");
    let swapped = std::fs::write(&new, binary)
        .and_then(|()| std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755)))
        .and_then(|()| std::fs::rename(&new, exe));
    if swapped.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    swapped
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "sha256:0101010101010101010101010101010101010101010101010101010101010101";

    fn asset(digest: Option<&str>) -> Asset {
        Asset {
            name: "schwitzer-server-0.3.0-linux-x86_64.tar.gz".into(),
            browser_download_url: "https://example.org/s.tar.gz".into(),
            size: 7,
            digest: digest.map(Into::into),
        }
    }

    #[test]
    fn download_info_reads_the_sha256_digest() {
        assert_eq!(
            asset(Some(DIGEST)).download_info(),
            Ok(Download {
                url: "https://example.org/s.tar.gz".into(),
                size: 7,
                sha256: [1; 32],
            })
        );
    }

    #[test]
    fn download_info_names_a_missing_or_malformed_digest() {
        for digest in [None, Some("sha256:zz"), Some("md5:0101"), Some("sha256:01")] {
            assert_eq!(
                asset(digest).download_info(),
                Err("schwitzer-server-0.3.0-linux-x86_64.tar.gz has no sha256 digest".into()),
                "{digest:?}"
            );
        }
    }

    #[test]
    fn tags_name_stable_or_pre_release_versions() {
        let version = |tag| parse_tag(tag).unwrap();
        assert_eq!(version("v1.2.3"), semver::Version::new(1, 2, 3));
        assert_eq!(version("1.2.3"), semver::Version::new(1, 2, 3));
        assert_eq!(parse_tag("nightly"), None);
        assert!(is_stable(&version("v1.2.3")));
        assert!(!is_stable(&version("v1.2.3-rc.1")));
    }

    #[test]
    fn verify_checks_size_and_digest() {
        let bytes = b"program";
        let sha256: [u8; 32] = Sha256::digest(bytes).into();
        let size = bytes.len() as u64;
        assert_eq!(verify(bytes, size, &sha256), Ok(()));
        assert_eq!(
            verify(bytes, size + 1, &sha256),
            Err("download is 7 bytes, expected 8".into())
        );
        let mut other = sha256;
        other[0] ^= 1;
        assert_eq!(
            verify(bytes, size, &other),
            Err("download does not match its sha256 digest".into())
        );
        assert!(verify(b"programs", size, &sha256).is_err());
    }

    #[cfg(unix)]
    fn tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn extract_finds_the_member_at_any_depth() {
        let archive = tar_gz(&[
            ("README", b"read me"),
            ("Schwitzer.app/Contents/MacOS/schwitzer", b"program"),
        ]);
        assert_eq!(
            extract_tar_gz(&archive, "schwitzer", 1 << 10),
            Ok(b"program".to_vec())
        );
        assert_eq!(
            extract_tar_gz(&archive, "schwitzer-server", 1 << 10),
            Err("the archive holds no schwitzer-server".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn replace_swaps_the_program() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("schwitzer");
        std::fs::write(&exe, b"old").unwrap();
        replace(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert!(!sibling(&exe, ".new").exists());
        let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}
