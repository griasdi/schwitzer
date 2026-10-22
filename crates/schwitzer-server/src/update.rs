//! `--auto-update`: installs the newest stable release's server binary for this platform over the
//! running program file and restarts into it in place (exec, same PID), so a supervisor such as
//! systemd sees no exit.

use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::release::Stable;

/// The platform name in the server's release assets; `None` where no server release is built.
pub const PLATFORM: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-x86_64")
} else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
    Some("linux-aarch64")
} else {
    None
};

const MAX_ARCHIVE: u64 = 64 << 20;
const BINARY: &str = "schwitzer-server";
/// How long a version whose install failed is left alone before it is tried again.
pub(crate) const RETRY_AFTER: Duration = Duration::from_secs(30 * 60);

/// This build's version.
pub(crate) fn current() -> semver::Version {
    env!("CARGO_PKG_VERSION")
        .parse()
        .expect("CARGO_PKG_VERSION is semver")
}

/// A download of a GitHub release, as the release list shows it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Asset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
    pub(crate) size: u64,
    pub(crate) digest: Option<String>,
}

fn asset_name(version: &semver::Version, platform: &str) -> String {
    format!("schwitzer-server-{version}-{platform}.tar.gz")
}

/// The archive to install, and what it must be.
#[derive(Debug, PartialEq)]
struct Download {
    url: String,
    size: u64,
    sha256: [u8; 32],
}

/// The server archive for `platform` among the `assets` of release `version`.
fn pick(assets: &[Asset], version: &semver::Version, platform: &str) -> Result<Download, String> {
    let name = asset_name(version, platform);
    let asset = assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| format!("release v{version} has no {name}"))?;
    let mut sha256 = [0; 32];
    asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .and_then(|h| hex::decode_to_slice(h, &mut sha256).ok())
        .ok_or_else(|| format!("{name} has no sha256 digest"))?;
    Ok(Download {
        url: asset.browser_download_url.clone(),
        size: asset.size,
        sha256,
    })
}

fn download(release: &Download) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .user_agent(format!("schwitzer-server/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut response = agent
        .get(&release.url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_ARCHIVE)
        .read_to_vec()
        .map_err(|e| format!("download failed: {e}"))
}

fn verify(bytes: &[u8], release: &Download) -> Result<(), String> {
    let got = bytes.len() as u64;
    if got != release.size {
        return Err(format!(
            "download is {got} bytes, expected {}",
            release.size
        ));
    }
    if Sha256::digest(bytes).as_slice() != release.sha256 {
        return Err("download does not match its sha256 digest".into());
    }
    Ok(())
}

fn unreadable(e: impl std::fmt::Display) -> String {
    format!("unreadable archive: {e}")
}

/// The program file in the release archive `bytes`: a regular file named `schwitzer-server` at
/// any depth.
fn extract(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries().map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let is_binary = entry.header().entry_type().is_file()
            && entry.path().map_err(unreadable)?.file_name() == Some(OsStr::new(BINARY));
        if is_binary {
            let mut binary = Vec::new();
            entry
                .take(MAX_ARCHIVE)
                .read_to_end(&mut binary)
                .map_err(unreadable)?;
            return Ok(binary);
        }
    }
    Err(format!("the archive holds no {BINARY}"))
}

/// `exe` with `suffix` appended to its file name.
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut path = exe.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

/// Writes `binary` beside `exe` and swaps it in. A rename gives a new file, so the running
/// process keeps the old one.
#[cfg(unix)]
fn replace(exe: &Path, binary: &[u8]) -> io::Result<()> {
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

#[cfg(not(unix))]
fn replace(_exe: &Path, _binary: &[u8]) -> io::Result<()> {
    Err(io::Error::other("auto update needs Unix"))
}

/// Installs new releases over this server's program file and restarts into them.
#[derive(Debug, Clone)]
pub struct AutoUpdate {
    exe: PathBuf,
    platform: &'static str,
}

impl AutoUpdate {
    /// Checks at startup what an update needs later: a server release for this platform and a
    /// program folder this process may write to.
    pub fn prepare() -> anyhow::Result<AutoUpdate> {
        let Some(platform) = PLATFORM else {
            bail!("no server release is built for this platform");
        };
        // Taken now: after the swap Linux reports the old path with " (deleted)".
        let exe = std::env::current_exe().context("locating the server program")?;
        let probe = sibling(&exe, ".new");
        std::fs::write(&probe, b"")
            .and_then(|()| std::fs::remove_file(&probe))
            .with_context(|| format!("cannot write beside {}", exe.display()))?;
        Ok(AutoUpdate { exe, platform })
    }

    /// Downloads `release`'s server for this platform, verifies it and puts it in place of the
    /// program file. Blocks.
    pub(crate) fn install(&self, release: &Stable) -> Result<(), String> {
        let download = pick(&release.assets, &release.version, self.platform)?;
        let bytes = self::download(&download)?;
        verify(&bytes, &download)?;
        let binary = extract(&bytes)?;
        replace(&self.exe, &binary)
            .map_err(|e| format!("cannot replace {}: {e}", self.exe.display()))
    }

    /// Replaces this process with the program file, with the same arguments and environment.
    /// Returns only on failure.
    #[cfg(unix)]
    pub fn restart(&self) -> io::Error {
        use std::os::unix::process::CommandExt;

        // std, tokio and SQLite open their descriptors close-on-exec, and the caller closed the
        // socket before, so the new program inherits nothing but stdio.
        std::process::Command::new(&self.exe)
            .args(std::env::args_os().skip(1))
            .exec()
    }

    /// Replaces this process with the program file. Returns only on failure.
    #[cfg(not(unix))]
    pub fn restart(&self) -> io::Error {
        io::Error::other("auto update needs Unix")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "sha256:0101010101010101010101010101010101010101010101010101010101010101";

    fn asset(name: &str, digest: Option<&str>) -> Asset {
        Asset {
            name: name.into(),
            browser_download_url: format!("https://example.org/{name}"),
            size: 7,
            digest: digest.map(Into::into),
        }
    }

    fn v(s: &str) -> semver::Version {
        s.parse().unwrap()
    }

    fn tar_gz(path: &str, data: &[u8]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, path, data).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn pick_finds_the_platform_asset_and_its_digest() {
        let assets = [
            asset("schwitzer-0.3.0-linux-x86_64.tar.gz", Some(DIGEST)),
            asset("schwitzer-server-0.3.0-linux-aarch64.tar.gz", Some(DIGEST)),
            asset("schwitzer-server-0.3.0-linux-x86_64.tar.gz", Some(DIGEST)),
        ];
        let got = pick(&assets, &v("0.3.0"), "linux-x86_64").unwrap();
        assert_eq!(
            got,
            Download {
                url: "https://example.org/schwitzer-server-0.3.0-linux-x86_64.tar.gz".into(),
                size: 7,
                sha256: [1; 32],
            }
        );
    }

    #[test]
    fn pick_names_a_missing_asset_or_digest() {
        let name = "schwitzer-server-0.3.0-linux-x86_64.tar.gz";
        let missing = pick(
            &[asset(
                "schwitzer-server-0.2.0-linux-x86_64.tar.gz",
                Some(DIGEST),
            )],
            &v("0.3.0"),
            "linux-x86_64",
        );
        assert_eq!(missing, Err(format!("release v0.3.0 has no {name}")));
        for digest in [None, Some("sha256:zz"), Some("md5:0101"), Some("sha256:01")] {
            assert_eq!(
                pick(&[asset(name, digest)], &v("0.3.0"), "linux-x86_64"),
                Err(format!("{name} has no sha256 digest")),
                "{digest:?}"
            );
        }
    }

    #[test]
    fn extract_finds_the_binary_at_any_depth() {
        let archive = tar_gz("dir/schwitzer-server", b"program");
        assert_eq!(extract(&archive), Ok(b"program".to_vec()));
        let other = tar_gz("dir/schwitzer", b"app");
        assert_eq!(
            extract(&other),
            Err("the archive holds no schwitzer-server".into())
        );
    }

    #[test]
    fn verify_checks_size_and_digest() {
        let bytes = b"program";
        let mut download = Download {
            url: String::new(),
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        };
        assert_eq!(verify(bytes, &download), Ok(()));
        download.size += 1;
        assert!(verify(bytes, &download).is_err());
        download.size -= 1;
        download.sha256[0] ^= 1;
        assert_eq!(
            verify(bytes, &download),
            Err("download does not match its sha256 digest".into())
        );
    }
}
