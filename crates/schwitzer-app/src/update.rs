//! Self-update from the GitHub releases: check for a newer release, download its archive (or
//! AppImage) for this platform, verify it against the digest GitHub publishes, swap the program
//! file in place and start the new program.
//!
//! Only builds with the `self-update` feature use it ([`ENABLED`]); the module is compiled and
//! tested in every build.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use futures::channel::oneshot;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Whether this build updates itself: only the release builds from CI (`--features self-update`).
pub const ENABLED: bool = cfg!(feature = "self-update");
/// How often the app looks for a new release.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(300);
/// The version of this build.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const LATEST_URL: &str = "https://api.github.com/repos/griasdi/schwitzer/releases/latest";
/// Replaces LATEST_URL, for a local test feed.
const URL_ENV: &str = "SCHWITZER_UPDATE_URL";
const MAX_RELEASE_JSON: u64 = 1 << 20;
const MAX_ARCHIVE: u64 = 256 << 20;
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
struct Latest {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

/// The name of the release asset of `version` for `platform` and `package`.
pub fn asset_name(version: &semver::Version, platform: &str, package: Package) -> String {
    let ext = match package {
        Package::AppImage => "AppImage",
        Package::Archive => ARCHIVE_EXT,
    };
    format!("schwitzer-{version}-{platform}.{ext}")
}

/// The release in GitHub's "latest release" answer `json` when it is newer than `current` and
/// a stable version; its asset for `platform` and `package` must be there with a sha256 digest.
pub fn parse_latest(
    json: &[u8],
    current: &semver::Version,
    platform: &str,
    package: Package,
) -> Result<Option<Release>, String> {
    let latest: Latest =
        serde_json::from_slice(json).map_err(|e| format!("unreadable release list: {e}"))?;
    if latest.draft || latest.prerelease {
        return Ok(None);
    }
    let tag = latest.tag_name.as_str();
    let version: semver::Version = tag
        .strip_prefix('v')
        .unwrap_or(tag)
        .parse()
        .map_err(|_| format!("release tag {tag} is not a version"))?;
    if !version.pre.is_empty() || version <= *current {
        return Ok(None);
    }
    let name = asset_name(&version, platform, package);
    let asset = latest
        .assets
        .into_iter()
        .find(|a| a.name == name)
        .ok_or_else(|| format!("release {tag} has no {name}"))?;
    let mut sha256 = [0; 32];
    asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .and_then(|h| hex::decode_to_slice(h, &mut sha256).ok())
        .ok_or_else(|| format!("{name} has no sha256 digest"))?;
    Ok(Some(Release {
        version,
        url: asset.browser_download_url,
        size: asset.size,
        sha256,
    }))
}

/// Asks GitHub for the latest release; `None` when there is nothing newer for this platform.
pub async fn check(package: Package) -> Result<Option<Release>, String> {
    let Some(platform) = PLATFORM else {
        return Err("No release is built for this platform".into());
    };
    blocking(move || fetch_latest(platform, package)).await
}

/// Downloads `release`, verifies it and puts it in place of `program`.
pub async fn install(release: Release, program: Program) -> Result<semver::Version, String> {
    blocking(move || {
        let bytes = download(&release)?;
        verify(&bytes, &release)?;
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

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(format!("schwitzer/{CURRENT}"))
        .build()
        .into()
}

fn fetch_latest(platform: &str, package: Package) -> Result<Option<Release>, String> {
    let url = std::env::var(URL_ENV).unwrap_or_else(|_| LATEST_URL.to_owned());
    let response = agent(Duration::from_secs(30))
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call();
    let mut response = match response {
        Ok(response) => response,
        // The repository has no release yet.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(format!("GitHub: {e}")),
    };
    let json = response
        .body_mut()
        .with_config()
        .limit(MAX_RELEASE_JSON)
        .read_to_vec()
        .map_err(|e| format!("GitHub: {e}"))?;
    let current = CURRENT.parse().expect("CARGO_PKG_VERSION is semver");
    parse_latest(&json, &current, platform, package)
}

fn download(release: &Release) -> Result<Vec<u8>, String> {
    let mut response = agent(Duration::from_secs(600))
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

fn verify(bytes: &[u8], release: &Release) -> Result<(), String> {
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

/// The program file in the release archive `bytes`.
#[cfg(not(windows))]
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

/// The program file in the release archive `bytes`.
#[cfg(windows)]
fn extract(bytes: &[u8]) -> Result<Vec<u8>, String> {
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

/// `exe` with `suffix` appended to its file name.
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut path = exe.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

/// Writes `binary` beside `exe` and swaps it in. A rename gives a new file, so the running
/// process keeps the old one, and macOS checks the new file's signature afresh.
#[cfg(not(windows))]
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

    fn latest(tag: &str, asset: &str, digest: Option<&str>) -> Vec<u8> {
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
        let json = latest(
            "v1.2.0",
            &asset_name(&v("1.2.0"), PLATFORM, Package::Archive),
            Some(DIGEST),
        );
        let release = parse_latest(&json, &v("1.1.9"), PLATFORM, Package::Archive)
            .unwrap()
            .unwrap();
        assert_eq!(release.version, v("1.2.0"));
        assert_eq!(release.url, "https://x/a");
        assert_eq!(release.size, 42);
        assert_eq!(hex::encode(release.sha256), &DIGEST[7..]);
    }

    #[test]
    fn same_or_older_release_is_no_update() {
        let json = latest(
            "v1.2.0",
            &asset_name(&v("1.2.0"), PLATFORM, Package::Archive),
            Some(DIGEST),
        );
        assert_eq!(
            parse_latest(&json, &v("1.2.0"), PLATFORM, Package::Archive),
            Ok(None)
        );
        assert_eq!(
            parse_latest(&json, &v("2.0.0"), PLATFORM, Package::Archive),
            Ok(None)
        );
    }

    #[test]
    fn drafts_and_prereleases_are_no_update() {
        let name = asset_name(&v("1.0.0"), PLATFORM, Package::Archive);
        for flag in ["draft", "prerelease"] {
            let mut json: serde_json::Value =
                serde_json::from_slice(&latest("v1.0.0", &name, Some(DIGEST))).unwrap();
            json[flag] = true.into();
            let json = json.to_string();
            assert_eq!(
                parse_latest(json.as_bytes(), &v("0.1.0"), PLATFORM, Package::Archive),
                Ok(None)
            );
        }
        let rc = latest(
            "v1.0.0-rc.1",
            &asset_name(&v("1.0.0-rc.1"), PLATFORM, Package::Archive),
            Some(DIGEST),
        );
        assert_eq!(
            parse_latest(&rc, &v("0.1.0"), PLATFORM, Package::Archive),
            Ok(None)
        );
    }

    #[test]
    fn missing_archive_is_an_error_naming_it() {
        let json = latest("v1.0.0", "schwitzer-1.0.0-other.tar.gz", Some(DIGEST));
        let error = parse_latest(&json, &v("0.1.0"), PLATFORM, Package::Archive).unwrap_err();
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
            let json = latest("v1.0.0", &name, digest);
            assert!(parse_latest(&json, &v("0.1.0"), PLATFORM, Package::Archive).is_err());
        }
    }

    #[test]
    fn tag_that_is_no_version_is_an_error() {
        let json = latest("nightly", "x", Some(DIGEST));
        assert!(parse_latest(&json, &v("0.1.0"), PLATFORM, Package::Archive).is_err());
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
            parse_latest(json.as_bytes(), &v("1.1.0"), PLATFORM, package)
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

    #[test]
    fn verify_checks_size_and_digest() {
        let bytes = b"archive";
        let release = Release {
            version: v("1.0.0"),
            url: String::new(),
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        };
        assert_eq!(verify(bytes, &release), Ok(()));
        assert!(verify(b"archivx", &release).is_err());
        assert!(verify(b"archive!", &release).is_err());
    }

    #[cfg(not(windows))]
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

    #[cfg(not(windows))]
    #[test]
    fn extract_finds_the_program_at_any_depth() {
        let archive = tar_gz(&[
            ("README", b"read me"),
            ("Schwitzer.app/Contents/MacOS/schwitzer", b"program"),
        ]);
        assert_eq!(extract(&archive), Ok(b"program".to_vec()));
        let error = extract(&tar_gz(&[("README", b"read me")])).unwrap_err();
        assert!(error.contains(BINARY), "{error}");
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

    #[test]
    fn replace_swaps_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join(BINARY);
        std::fs::write(&exe, b"old").unwrap();
        replace(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert!(!sibling(&exe, ".new").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        #[cfg(windows)]
        assert_eq!(std::fs::read(sibling(&exe, ".old")).unwrap(), b"old");
    }
}
