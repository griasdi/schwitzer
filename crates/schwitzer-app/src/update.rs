//! Self-update to the release the app's servers announced, or GitHub's latest release when no
//! server announced one: fetch that release from GitHub, download its archive (or AppImage) for
//! this platform, verify it against the digest GitHub publishes, swap the program file in place
//! and start the new program.
//!
//! Only builds with the `self-update` feature use it ([`ENABLED`]); the module is compiled and
//! tested in every build.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use futures::channel::oneshot;
#[cfg(unix)]
use schwitzer_release::replace;
use schwitzer_release::{ApiError, Asset, PLATFORM, RELEASES_URL, sibling};
use serde::Deserialize;

/// Whether this build updates itself: only the release builds from CI (`--features self-update`).
pub const ENABLED: bool = cfg!(feature = "self-update");
/// How often the app looks for a new release.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(300);
/// The version of this build.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Replaces [`RELEASES_URL`], for a local test feed.
const URL_ENV: &str = "SCHWITZER_UPDATE_URL";
const MAX_RELEASE_JSON: u64 = 1 << 20;
const MAX_ARCHIVE: u64 = 256 << 20;
const USER_AGENT: &str = concat!("schwitzer/", env!("CARGO_PKG_VERSION"));
const ARCHIVE_EXT: &str = if cfg!(windows) { "zip" } else { "tar.gz" };
/// The file the updater takes out of the archive, at any depth.
const BINARY: &str = if cfg!(windows) {
    "schwitzer.exe"
} else {
    "schwitzer"
};

/// How this copy of the program came, which decides the release asset it updates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    /// The `.tar.gz` or `.zip` archive (the Windows installer installs the same `schwitzer.exe`).
    Archive,
    /// The Linux AppImage: the whole `.AppImage` file is the program file.
    AppImage,
}

/// The file an update replaces and the restart runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub path: PathBuf,
    pub package: Package,
}

impl Program {
    /// This program's file. Inside an AppImage the program runs from a read-only mount
    /// (`APPDIR`); the AppImage runtime names the file it runs from in `APPIMAGE`.
    pub fn current() -> Option<Program> {
        Self::locate(
            std::env::current_exe().ok(),
            std::env::var_os("APPIMAGE"),
            std::env::var_os("APPDIR"),
        )
    }

    fn locate(
        exe: Option<PathBuf>,
        appimage: Option<OsString>,
        appdir: Option<OsString>,
    ) -> Option<Program> {
        if cfg!(target_os = "linux")
            && let (Some(appimage), Some(appdir), Some(exe)) = (&appimage, &appdir, &exe)
            && !appimage.is_empty()
            && !appdir.is_empty()
            // A process started from an AppImage inherits APPIMAGE but does not run under its APPDIR.
            && exe.starts_with(appdir)
        {
            return Some(Program {
                path: appimage.into(),
                package: Package::AppImage,
            });
        }
        exe.map(|path| Program {
            path,
            package: Package::Archive,
        })
    }
}

/// A release newer than this build, with the archive for this platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: semver::Version,
    pub url: String,
    pub size: u64,
    pub sha256: [u8; 32],
}

/// The version of this build, parsed.
pub fn current() -> semver::Version {
    CURRENT.parse().expect("CARGO_PKG_VERSION is semver")
}

/// Where the updater stands, as the settings page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Failed(String),
    /// Found; not installed because auto-install is off or the install failed (`error`).
    Available {
        release: Release,
        error: Option<String>,
    },
    Installing(Release),
    /// Installed on disk; the running process is still the old version.
    Installed(semver::Version),
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<Asset>,
}

/// The name of the release asset of `version` for `platform` and `package`.
pub fn asset_name(version: &semver::Version, platform: &str, package: Package) -> String {
    let ext = match package {
        Package::AppImage => "AppImage",
        Package::Archive => ARCHIVE_EXT,
    };
    format!("schwitzer-{version}-{platform}.{ext}")
}

/// The release in GitHub's answer `json` for one release when it is newer than `current` and a
/// stable version; its asset for `platform` and `package` must be there with a sha256 digest.
pub fn parse_release(
    json: &[u8],
    current: &semver::Version,
    platform: &str,
    package: Package,
) -> Result<Option<Release>, String> {
    let latest: GitHubRelease =
        serde_json::from_slice(json).map_err(|e| format!("unreadable release: {e}"))?;
    if latest.draft || latest.prerelease {
        return Ok(None);
    }
    let tag = latest.tag_name.as_str();
    let version = schwitzer_release::parse_tag(tag)
        .ok_or_else(|| format!("release tag {tag} is not a version"))?;
    if !schwitzer_release::is_stable(&version) || version <= *current {
        return Ok(None);
    }
    let name = asset_name(&version, platform, package);
    let asset = latest
        .assets
        .into_iter()
        .find(|a| a.name == name)
        .ok_or_else(|| format!("release {tag} has no {name}"))?;
    let download = asset.download_info()?;
    Ok(Some(Release {
        version,
        url: download.url,
        size: download.size,
        sha256: download.sha256,
    }))
}

/// Asks GitHub for release `version`, which a server announced, or for its latest release when no
/// server announced one; `None` when it is not newer than this build.
pub async fn check(
    version: Option<semver::Version>,
    package: Package,
) -> Result<Option<Release>, String> {
    let Some(platform) = PLATFORM else {
        return Err("No release is built for this platform".into());
    };
    blocking(move || fetch_release(version, platform, package)).await
}

/// Downloads `release`, verifies it and puts it in place of `program`.
pub async fn install(release: Release, program: Program) -> Result<semver::Version, String> {
    blocking(move || {
        let bytes = schwitzer_release::download(
            USER_AGENT,
            Duration::from_secs(600),
            &release.url,
            MAX_ARCHIVE,
        )?;
        schwitzer_release::verify(&bytes, release.size, &release.sha256)?;
        let binary = match program.package {
            Package::AppImage => bytes,
            Package::Archive => extract(&bytes)?,
        };
        replace(&program.path, &binary)
            .map_err(|e| format!("cannot replace {}: {e}", program.path.display()))?;
        Ok(release.version)
    })
    .await
}

/// Removes what an update left beside `exe`: an unfinished `<exe>.new`, the replaced `<exe>.old`.
pub fn clean_leftovers(exe: &Path) {
    let _ = std::fs::remove_file(sibling(exe, ".new"));
    let _ = std::fs::remove_file(sibling(exe, ".old"));
}

/// Starts `exe` with this process's arguments and environment; the caller then exits.
pub fn restart(exe: &Path) -> io::Result<()> {
    // The new program gets no file descriptor this process inherited (Rust opens its own ones
    // close-on-exec): inside an AppImage the runtime's keepalive pipe and mount directory would
    // keep the old mount and its FUSE process alive until the new program exits.
    #[cfg(unix)]
    close_fds::set_fds_cloexec_threadsafe(3, &[]);
    Command::new(exe)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map(drop)
}

/// Runs `f` on its own thread: ureq blocks, and iced's executor need not be tokio.
fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> impl Future<Output = Result<T, String>> {
    let (tx, rx) = oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("schwitzer-update".into())
        .spawn(move || {
            let _ = tx.send(f());
        });
    async move {
        spawned.map_err(|e| format!("cannot start the update worker: {e}"))?;
        rx.await
            .unwrap_or_else(|_| Err("update worker stopped".into()))
    }
}

fn fetch_release(
    version: Option<semver::Version>,
    platform: &str,
    package: Package,
) -> Result<Option<Release>, String> {
    let base = std::env::var(URL_ENV).unwrap_or_else(|_| RELEASES_URL.to_owned());
    let url = match &version {
        Some(v) => format!("{base}/tags/v{v}"),
        None => format!("{base}/latest"),
    };
    let json = schwitzer_release::api_get(
        USER_AGENT,
        Duration::from_secs(30),
        &url,
        &[],
        MAX_RELEASE_JSON,
    )
    .map_err(|e| match e {
        ApiError::NotFound(_) => match version {
            Some(v) => format!("release v{v} is not on GitHub"),
            None => "no release is on GitHub".to_owned(),
        },
        ApiError::Failed(message) => message,
    })?;
    parse_release(&json, &current(), platform, package)
}

/// The program file in the release archive `bytes`.
#[cfg(unix)]
fn extract(bytes: &[u8]) -> Result<Vec<u8>, String> {
    schwitzer_release::extract_tar_gz(bytes, BINARY, MAX_ARCHIVE)
}

/// The program file in the release archive `bytes`.
#[cfg(windows)]
fn extract(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::ffi::OsStr;
    use std::io::Read;

    use schwitzer_release::unreadable;

    let mut archive = zip::ZipArchive::new(io::Cursor::new(bytes)).map_err(unreadable)?;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(unreadable)?;
        let is_binary = file.is_file()
            && file
                .enclosed_name()
                .is_some_and(|path| path.file_name() == Some(OsStr::new(BINARY)));
        if is_binary {
            let mut binary = Vec::new();
            file.take(MAX_ARCHIVE)
                .read_to_end(&mut binary)
                .map_err(unreadable)?;
            return Ok(binary);
        }
    }
    Err(format!("the archive holds no {BINARY}"))
}

/// Writes `binary` beside `exe` and swaps it in. A running program may be renamed but not
/// deleted on Windows, so the old one moves to `<exe>.old` until the next start removes it.
#[cfg(windows)]
fn replace(exe: &Path, binary: &[u8]) -> io::Result<()> {
    let new = sibling(exe, ".new");
    let old = sibling(exe, ".old");
    std::fs::write(&new, binary)?;
    match std::fs::remove_file(&old) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::fs::rename(exe, &old)?;
    if let Err(e) = std::fs::rename(&new, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORM: &str = "linux-x86_64";
    const DIGEST: &str = "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn v(s: &str) -> semver::Version {
        s.parse().unwrap()
    }

    fn release_json(tag: &str, asset: &str, digest: Option<&str>) -> Vec<u8> {
        serde_json::json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": false,
            "assets": [
                {"name": "notes.txt", "browser_download_url": "https://x/notes", "size": 1, "digest": null},
                {"name": asset, "browser_download_url": "https://x/a", "size": 42, "digest": digest},
            ],
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn newer_release_is_found_with_its_archive() {
        let json = release_json(
            "v1.2.0",
            &asset_name(&v("1.2.0"), PLATFORM, Package::Archive),
            Some(DIGEST),
        );
        let release = parse_release(&json, &v("1.1.9"), PLATFORM, Package::Archive)
            .unwrap()
            .unwrap();
        assert_eq!(release.version, v("1.2.0"));
        assert_eq!(release.url, "https://x/a");
        assert_eq!(release.size, 42);
        assert_eq!(hex::encode(release.sha256), &DIGEST[7..]);
    }

    #[test]
    fn same_or_older_release_is_no_update() {
        let json = release_json(
            "v1.2.0",
            &asset_name(&v("1.2.0"), PLATFORM, Package::Archive),
            Some(DIGEST),
        );
        assert_eq!(
            parse_release(&json, &v("1.2.0"), PLATFORM, Package::Archive),
            Ok(None)
        );
        assert_eq!(
            parse_release(&json, &v("2.0.0"), PLATFORM, Package::Archive),
            Ok(None)
        );
    }

    #[test]
    fn drafts_and_prereleases_are_no_update() {
        let name = asset_name(&v("1.0.0"), PLATFORM, Package::Archive);
        for flag in ["draft", "prerelease"] {
            let mut json: serde_json::Value =
                serde_json::from_slice(&release_json("v1.0.0", &name, Some(DIGEST))).unwrap();
            json[flag] = true.into();
            let json = json.to_string();
            assert_eq!(
                parse_release(json.as_bytes(), &v("0.1.0"), PLATFORM, Package::Archive),
                Ok(None)
            );
        }
        let rc = release_json(
            "v1.0.0-rc.1",
            &asset_name(&v("1.0.0-rc.1"), PLATFORM, Package::Archive),
            Some(DIGEST),
        );
        assert_eq!(
            parse_release(&rc, &v("0.1.0"), PLATFORM, Package::Archive),
            Ok(None)
        );
    }

    #[test]
    fn missing_archive_is_an_error_naming_it() {
        let json = release_json("v1.0.0", "schwitzer-1.0.0-other.tar.gz", Some(DIGEST));
        let error = parse_release(&json, &v("0.1.0"), PLATFORM, Package::Archive).unwrap_err();
        assert!(
            error.contains(&asset_name(&v("1.0.0"), PLATFORM, Package::Archive)),
            "{error}"
        );
    }

    #[test]
    fn missing_or_malformed_digest_is_an_error() {
        let name = asset_name(&v("1.0.0"), PLATFORM, Package::Archive);
        let md5 = DIGEST.replace("sha256", "md5");
        for digest in [None, Some("sha256:abc"), Some(md5.as_str())] {
            let json = release_json("v1.0.0", &name, digest);
            assert!(parse_release(&json, &v("0.1.0"), PLATFORM, Package::Archive).is_err());
        }
    }

    #[test]
    fn tag_that_is_no_version_is_an_error() {
        let json = release_json("nightly", "x", Some(DIGEST));
        assert!(parse_release(&json, &v("0.1.0"), PLATFORM, Package::Archive).is_err());
    }

    #[test]
    fn appimage_updates_from_the_appimage_asset() {
        let archive = asset_name(&v("1.2.0"), PLATFORM, Package::Archive);
        let json = serde_json::json!({
            "tag_name": "v1.2.0",
            "draft": false,
            "prerelease": false,
            "assets": [
                {"name": archive, "browser_download_url": "https://x/a", "size": 42, "digest": DIGEST},
                {"name": "schwitzer-1.2.0-linux-x86_64.AppImage", "browser_download_url": "https://x/img", "size": 43, "digest": DIGEST},
            ],
        })
        .to_string();
        let found = |package| {
            parse_release(json.as_bytes(), &v("1.1.0"), PLATFORM, package)
                .unwrap()
                .unwrap()
                .url
        };
        assert_eq!(found(Package::AppImage), "https://x/img");
        assert_eq!(found(Package::Archive), "https://x/a");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_is_found_only_under_its_mount() {
        let appimage = Some(OsString::from("/home/u/S.AppImage"));
        let appdir = Some(OsString::from("/tmp/.mount_x"));
        assert_eq!(
            Program::locate(
                Some("/tmp/.mount_x/usr/bin/schwitzer".into()),
                appimage.clone(),
                appdir.clone()
            ),
            Some(Program {
                path: "/home/u/S.AppImage".into(),
                package: Package::AppImage,
            })
        );
        let outside = Some(Program {
            path: "/usr/bin/schwitzer".into(),
            package: Package::Archive,
        });
        assert_eq!(
            Program::locate(Some("/usr/bin/schwitzer".into()), appimage, appdir.clone()),
            outside
        );
        assert_eq!(
            Program::locate(Some("/usr/bin/schwitzer".into()), None, appdir),
            outside
        );
    }

    #[cfg(windows)]
    #[test]
    fn extract_finds_the_program_in_the_zip() {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("README", options).unwrap();
        zip.write_all(b"read me").unwrap();
        zip.start_file("schwitzer.exe", options).unwrap();
        zip.write_all(b"program").unwrap();
        let archive = zip.finish().unwrap().into_inner();
        assert_eq!(extract(&archive), Ok(b"program".to_vec()));
    }

    #[cfg(windows)]
    #[test]
    fn replace_swaps_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join(BINARY);
        std::fs::write(&exe, b"old").unwrap();
        replace(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert!(!sibling(&exe, ".new").exists());
        assert_eq!(std::fs::read(sibling(&exe, ".old")).unwrap(), b"old");
    }
}
