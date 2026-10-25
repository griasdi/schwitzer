//! `--auto-update`: installs the newest server binary of the channel it follows (stable releases,
//! or the rolling `dev` prerelease) for this platform over the running program file and restarts
//! into it in place (exec, same PID), so a supervisor such as systemd sees no exit.

use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use schwitzer_release::{Asset, Channel, Download, sibling};

use crate::user_agent;

/// The platform name in the server's release assets; `None` where no server release is built.
fn platform() -> Option<&'static str> {
    schwitzer_release::PLATFORM.filter(|platform| platform.starts_with("linux-"))
}

const MAX_ARCHIVE: u64 = 64 << 20;
const BINARY: &str = "schwitzer-server";

fn asset_name(version: &semver::Version, platform: &str) -> String {
    format!("schwitzer-server-{version}-{platform}.tar.gz")
}

/// The server archive for `platform` among the `assets` of release `version`.
fn pick(assets: &[Asset], version: &semver::Version, platform: &str) -> Result<Download, String> {
    let name = asset_name(version, platform);
    assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| format!("release v{version} has no {name}"))?
        .download_info()
}

/// Takes the server program out of the release archive `bytes` and swaps it in for `exe`.
#[cfg(unix)]
fn unpack_and_replace(exe: &Path, bytes: &[u8]) -> Result<(), String> {
    let binary = schwitzer_release::extract_tar_gz(bytes, BINARY, MAX_ARCHIVE)?;
    schwitzer_release::replace(exe, &binary)
        .map_err(|e| format!("cannot replace {}: {e}", exe.display()))
}

#[cfg(not(unix))]
fn unpack_and_replace(exe: &Path, _bytes: &[u8]) -> Result<(), String> {
    Err(format!(
        "cannot replace {}: auto update needs Unix",
        exe.display()
    ))
}

/// Installs new releases over this server's program file and restarts into them.
#[derive(Debug, Clone)]
pub struct AutoUpdate {
    exe: PathBuf,
    platform: &'static str,
    channel: Channel,
}

impl AutoUpdate {
    /// Checks at startup what an update needs later: a server release for this platform and a
    /// program folder this process may write to. Updates follow `channel`.
    pub fn prepare(channel: Channel) -> anyhow::Result<AutoUpdate> {
        let Some(platform) = platform() else {
            bail!("no server release is built for this platform");
        };
        // Taken now: after the swap Linux reports the old path with " (deleted)".
        let exe = std::env::current_exe().context("locating the server program")?;
        let probe = sibling(&exe, ".new");
        std::fs::write(&probe, b"")
            .and_then(|()| std::fs::remove_file(&probe))
            .with_context(|| format!("cannot write beside {}", exe.display()))?;
        Ok(AutoUpdate {
            exe,
            platform,
            channel,
        })
    }

    /// The channel the updates follow.
    pub(crate) fn channel(&self) -> Channel {
        self.channel
    }

    /// The platform name in the server's release assets.
    pub(crate) fn platform(&self) -> &'static str {
        self.platform
    }

    /// Downloads the server of release `version` for this platform from its `assets`, verifies it
    /// and puts it in place of the program file. Blocks.
    pub(crate) fn install(
        &self,
        version: &semver::Version,
        assets: &[Asset],
    ) -> Result<(), String> {
        let download = pick(assets, version, self.platform)?;
        let bytes = schwitzer_release::fetch(&user_agent(), &download, MAX_ARCHIVE)?;
        unpack_and_replace(&self.exe, &bytes)
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
        assert_eq!(
            pick(&[asset(name, None)], &v("0.3.0"), "linux-x86_64"),
            Err(format!("{name} has no sha256 digest"))
        );
    }
}
