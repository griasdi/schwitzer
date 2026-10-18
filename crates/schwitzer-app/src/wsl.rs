//! The app may run on Windows while omp and the repos live in a WSL distro. This module maps
//! paths between the two and finds the running distros.
//!
//! Only distros that `wsl.exe --list --running` reports are touched by periodic work: reading
//! `\\wsl.localhost\<distro>` starts a stopped distro.

use std::path::{Path, PathBuf};

use iced::Subscription;

/// The prefix under which Windows shows the files of a WSL distro.
const UNC: &str = r"\\wsl.localhost\";

/// The older form of [`UNC`].
const OLD_UNC: &str = r"\\wsl$\";

/// How often the running distros are listed.
#[cfg(windows)]
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// A running WSL distro: its home folders and its default projects folders.
#[cfg_attr(not(windows), allow(dead_code))] // built only on Windows
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distro {
    pub name: String,
    /// `\\wsl.localhost\<name>\home\<user>`, sorted.
    pub homes: Vec<PathBuf>,
    /// `code`, `projects` and `work` under the homes that exist: entered into the projects
    /// folders the first time the distro is seen.
    pub projects_dirs: Vec<PathBuf>,
}

/// `s` without `prefix`, compared ASCII case-insensitively.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// What follows `\\wsl.localhost\` or `\\wsl$\` in `s`.
fn strip_unc(s: &str) -> Option<&str> {
    strip_prefix_ci(s, UNC).or_else(|| strip_prefix_ci(s, OLD_UNC))
}

/// The distro a `\\wsl.localhost\<distro>\…` (or `\\wsl$\…`) path lies in.
pub fn distro_of(path: &Path) -> Option<&str> {
    let rest = strip_unc(path.to_str()?)?;
    let name = rest.split('\\').next().unwrap_or_default();
    (!name.is_empty()).then_some(name)
}

/// The distro and the Linux path of a `\\wsl.localhost\<distro>\…` (or `\\wsl$\…`) path:
/// `\\wsl.localhost\Ubuntu\home\felix` is `("Ubuntu", "/home/felix")`. `None` for any
/// other path.
pub fn to_linux(path: &Path) -> Option<(&str, String)> {
    let rest = strip_unc(path.to_str()?)?;
    let (name, tail) = rest.split_once('\\').unwrap_or((rest, ""));
    if name.is_empty() {
        return None;
    }
    let linux = format!("/{}", tail.trim_end_matches('\\').replace('\\', "/"));
    Some((name, linux))
}

/// `path` with a WSL prefix in its one spelling, `\\wsl.localhost\`, so the same folder is
/// stored and compared once.
pub fn canonical(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(strip_unc) {
        Some(rest) => PathBuf::from(format!("{UNC}{rest}")),
        None => path,
    }
}

/// The Windows path of `linux`, a path in `distro`: `/mnt/c/…` is `C:\…`, anything else lies
/// under `\\wsl.localhost\<distro>`. `None` for a relative path.
pub fn to_windows(distro: &str, linux: &str) -> Option<String> {
    if !linux.starts_with('/') {
        return None;
    }
    if let Some(rest) = linux.strip_prefix("/mnt/") {
        let mut chars = rest.chars();
        if let Some(letter) = chars.next().filter(char::is_ascii_alphabetic) {
            let tail = chars.as_str();
            if tail.is_empty() || tail.starts_with('/') {
                let tail = tail.trim_start_matches('/').replace('/', "\\");
                return Some(format!("{}:\\{tail}", letter.to_ascii_uppercase()));
            }
        }
    }
    Some(format!("{UNC}{distro}{}", linux.replace('/', "\\")))
}

/// `path` as the plugin in `repo` sees it: a Linux path when the repo lies in a WSL distro
/// (`C:\…` is `/mnt/c/…`, a path in the same distro its Linux path), else `path` itself. A
/// path the distro cannot name stays as is.
pub fn path_for(repo: &Path, path: &Path) -> PathBuf {
    let Some(distro) = distro_of(repo) else {
        return path.to_owned();
    };
    let Some(dir) = path.to_str() else {
        return path.to_owned();
    };
    let bytes = dir.as_bytes();
    if bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || matches!(bytes[2], b'\\' | b'/'))
    {
        let letter = char::from(bytes[0].to_ascii_lowercase());
        let rest = dir[2..].trim_start_matches(['\\', '/']).replace('\\', "/");
        return if rest.is_empty() {
            PathBuf::from(format!("/mnt/{letter}"))
        } else {
            PathBuf::from(format!("/mnt/{letter}/{rest}"))
        };
    }
    if let Some((name, linux)) = to_linux(path)
        && name.eq_ignore_ascii_case(distro)
    {
        return PathBuf::from(linux);
    }
    path.to_owned()
}

/// The distro names in the output of `wsl.exe --list --quiet`: UTF-16LE when it writes to a
/// Windows process, plain text under WSL interop.
#[cfg(any(windows, test))]
fn parse_distros(out: &[u8]) -> Vec<String> {
    let text = if out.contains(&0) {
        let (pairs, _) = out.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(out).into_owned()
    };
    text.lines()
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The names of the running WSL distros; none when WSL is missing or nothing runs.
#[cfg(windows)]
pub fn running_distros() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    /// Keeps the GUI app from flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = Command::new("wsl.exe")
        .args(["--list", "--running", "--quiet"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match output {
        Ok(out) if out.status.success() => parse_distros(&out.stdout),
        _ => Vec::new(),
    }
}

/// The running distros with their homes and default projects folders.
#[cfg(windows)]
fn distros() -> Vec<Distro> {
    use crate::settings;

    running_distros()
        .into_iter()
        .map(|name| {
            let homes = settings::subdirs(Path::new(&format!("{UNC}{name}\\home")));
            let projects_dirs: Vec<PathBuf> = homes
                .iter()
                .flat_map(|home| settings::default_projects_dirs(home))
                .collect();
            Distro {
                name,
                homes,
                projects_dirs,
            }
        })
        .collect()
}

#[cfg(windows)]
fn watch() -> impl futures::Stream<Item = Vec<Distro>> {
    use futures::SinkExt;

    iced::stream::channel(4, async move |mut out| {
        let mut last: Option<Vec<Distro>> = None;
        let mut tick = tokio::time::interval(CHECK_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let Ok(found) = tokio::task::spawn_blocking(distros).await else {
                return;
            };
            if last.as_ref() != Some(&found) {
                last = Some(found.clone());
                if out.send(found).await.is_err() {
                    return;
                }
            }
        }
    })
}

/// The running WSL distros, emitted whenever they change.
#[cfg(windows)]
pub fn subscription() -> Subscription<Vec<Distro>> {
    Subscription::run(watch)
}

/// The running WSL distros: none outside Windows.
#[cfg(not(windows))]
pub fn subscription() -> Subscription<Vec<Distro>> {
    Subscription::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_paths_map_to_windows() {
        let win = |p| to_windows("Ubuntu", p);
        assert_eq!(
            win("/home/felix/code/x").as_deref(),
            Some(r"\\wsl.localhost\Ubuntu\home\felix\code\x")
        );
        assert_eq!(
            win("/mnt/c/Users/felix").as_deref(),
            Some(r"C:\Users\felix")
        );
        assert_eq!(win("/mnt/d").as_deref(), Some(r"D:\"));
        assert_eq!(
            win("/mnt/cd/x").as_deref(),
            Some(r"\\wsl.localhost\Ubuntu\mnt\cd\x")
        );
        assert_eq!(win("rel/x"), None);
        assert_eq!(win(""), None);
    }

    #[test]
    fn paths_as_seen_from_a_wsl_repo() {
        let repo = Path::new(r"\\wsl.localhost\Ubuntu\home\felix\code\x");
        assert_eq!(
            path_for(
                repo,
                Path::new(r"C:\Users\felix\AppData\Roaming\schwitzer\data")
            ),
            Path::new("/mnt/c/Users/felix/AppData/Roaming/schwitzer/data")
        );
        assert_eq!(path_for(repo, Path::new("C:")), Path::new("/mnt/c"));
        assert_eq!(
            path_for(repo, Path::new(r"\\wsl$\ubuntu\tmp\sw")),
            Path::new("/tmp/sw")
        );
        let other = Path::new(r"\\wsl.localhost\Debian\tmp");
        assert_eq!(path_for(repo, other), other);
        let data = Path::new(r"C:\Users\felix\data");
        assert_eq!(path_for(Path::new(r"C:\code\x"), data), data);
    }

    #[test]
    fn distro_of_a_path() {
        assert_eq!(
            distro_of(Path::new(r"\\wsl.localhost\Ubuntu\home")),
            Some("Ubuntu")
        );
        assert_eq!(distro_of(Path::new(r"\\WSL$\Debian")), Some("Debian"));
        assert_eq!(distro_of(Path::new(r"C:\x")), None);
        assert_eq!(distro_of(Path::new(r"\\wsl.localhost\")), None);
    }

    #[test]
    fn wsl_paths_as_linux() {
        let linux = |p: &str| to_linux(Path::new(p)).map(|(d, l)| (d.to_owned(), l));
        assert_eq!(
            linux(r"\\wsl.localhost\Ubuntu\home\felix\code"),
            Some(("Ubuntu".to_owned(), "/home/felix/code".to_owned()))
        );
        assert_eq!(
            linux(r"\\WSL$\Debian\tmp\"),
            Some(("Debian".to_owned(), "/tmp".to_owned()))
        );
        assert_eq!(
            linux(r"\\wsl.localhost\Ubuntu"),
            Some(("Ubuntu".to_owned(), "/".to_owned()))
        );
        assert_eq!(linux(r"\\wsl.localhost\"), None);
        assert_eq!(linux(r"C:\code"), None);
    }

    #[test]
    fn canonical_spelling() {
        assert_eq!(
            canonical(PathBuf::from(r"\\wsl$\Ubuntu\home\x")),
            Path::new(r"\\wsl.localhost\Ubuntu\home\x")
        );
        assert_eq!(
            canonical(PathBuf::from(r"\\WSL.LOCALHOST\Ubuntu\x")),
            Path::new(r"\\wsl.localhost\Ubuntu\x")
        );
        assert_eq!(canonical(PathBuf::from(r"C:\x")), Path::new(r"C:\x"));
    }

    #[test]
    fn distro_list_in_both_encodings() {
        let utf16: Vec<u8> = "\u{feff}Ubuntu\r\ndocker-desktop\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(parse_distros(&utf16), ["Ubuntu", "docker-desktop"]);
        assert_eq!(parse_distros(b"Ubuntu\r\n\r\n"), ["Ubuntu"]);
        assert!(parse_distros(b"").is_empty());
    }
}
