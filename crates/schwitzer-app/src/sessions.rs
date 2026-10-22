//! Live omp sessions of this machine, read from the session files.
//!
//! omp keeps one `<sessions dir>/<project>/<session>.jsonl` per session; subagents write
//! `<session>/<agent>.jsonl` next to it. A session is *active* when its file or one of its
//! subagent files changed in the last ten minutes. Only active sessions are parsed, and every
//! file incrementally: the scanner remembers the byte offset it has read up to.
//!
//! Several sessions directories are read: the configured one and, on Windows, the one of every
//! running WSL distro. The working directories of a WSL session are Linux paths; they are
//! mapped to the Windows paths that reach them, so they match the linked repos.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use futures::{SinkExt, Stream};
use iced::Subscription;
use schwitzer_proto::ClaimToken;
use serde_json::Value;

/// A session counts as active this long after its last write.
const ACTIVE_WINDOW: Duration = Duration::from_secs(10 * 60);

/// How often the session files are looked at.
const SCAN_EVERY: Duration = Duration::from_secs(5);

/// What an agent echoes after claiming a task: this text, then the 32 hex digits of the token.
const MARKER: &str = "schwitzer-claim: ";

/// customType of the omp plugin's task message (`omp-plugin/index.ts`, `TASK_MESSAGE`), which
/// carries the build claim of a session that executes an approved plan.
const TASK_MESSAGE: &str = "schwitzer-task";

/// Hex digits of a claim token.
const MARKER_HEX: usize = ClaimToken::LEN * 2;

/// An active omp session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSession {
    /// omp's session id.
    pub id: String,
    /// Latest title; empty until omp has named the session.
    pub title: String,
    /// The session's working directory, as omp recorded it; for a session in WSL, the Windows
    /// path to it.
    pub cwd: PathBuf,
    /// Last component of the session's working directory.
    pub project: String,
    /// Latest model.
    pub model: String,
    /// Start of the session, ms since the epoch; 0 when the file does not say.
    pub started_ms: u64,
    /// Latest write to the session or one of its subagents, ms since the epoch.
    pub active_ms: u64,
    /// Tokens of every assistant message, subagents included.
    pub tokens: u64,
    /// Claim tokens the session's prompts, tool results and the plugin's task messages carried,
    /// in order of appearance.
    pub markers: Vec<ClaimToken>,
}

/// A directory omp writes sessions into.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionsDir {
    pub path: PathBuf,
    /// The WSL distro whose omp writes here; its working directories are Linux paths in it.
    pub distro: Option<String>,
}

/// Identity of the watcher: other directories restart it.
#[derive(Clone)]
struct SessionsKey(Vec<SessionsDir>);

impl Hash for SessionsKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        0x7363_7373_u32.hash(h);
        self.0.hash(h);
    }
}

/// The active sessions under `dirs`, emitted whenever they change.
pub fn subscription(dirs: Vec<SessionsDir>) -> Subscription<Vec<LocalSession>> {
    Subscription::run_with(SessionsKey(dirs), watch)
}

/// The directories of `dirs` that may be read now: those of a WSL distro only while it runs,
/// since reading them would start a stopped one.
fn scannable(dirs: &[SessionsDir]) -> Vec<SessionsDir> {
    if dirs.iter().all(|dir| dir.distro.is_none()) {
        return dirs.to_vec();
    }
    let Some(running) = crate::wsl::running_distros() else {
        return dirs.to_vec();
    };
    dirs.iter()
        .filter(|dir| dir.distro.as_ref().is_none_or(|d| running.contains(d)))
        .cloned()
        .collect()
}

fn watch(key: &SessionsKey) -> impl Stream<Item = Vec<LocalSession>> + use<> {
    let dirs = key.0.clone();
    iced::stream::channel(4, async move |mut out| {
        let mut scanner = Scanner::default();
        let mut last = Vec::new();
        let mut tick = tokio::time::interval(SCAN_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let dirs = dirs.clone();
            let scanned = tokio::task::spawn_blocking(move || {
                let sessions = scanner.scan(&scannable(&dirs), schwitzer_proto::now_ms());
                (scanner, sessions)
            })
            .await;
            let Ok((back, sessions)) = scanned else {
                return;
            };
            scanner = back;
            if sessions != last {
                last.clone_from(&sessions);
                if out.send(sessions).await.is_err() {
                    return;
                }
            }
        }
    })
}

/// What is known of one session file.
#[derive(Default)]
struct FileState {
    /// Bytes read so far, partial line included.
    offset: u64,
    /// The unfinished last line.
    partial: Vec<u8>,
    id: String,
    title: String,
    cwd: String,
    model: String,
    started_ms: u64,
    tokens: u64,
    markers: Vec<ClaimToken>,
}

impl FileState {
    /// Reads what was appended to the file at `path` since the last call. `main` is false for
    /// subagent files, which add tokens only.
    fn update(&mut self, path: &Path, main: bool) -> std::io::Result<()> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            *self = Self::default();
        }
        if len == self.offset {
            return Ok(());
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut chunk = Vec::new();
        file.read_to_end(&mut chunk)?;
        self.offset += chunk.len() as u64;
        self.partial.extend_from_slice(&chunk);
        let Some(end) = self.partial.iter().rposition(|&b| b == b'\n') else {
            return Ok(());
        };
        let rest = self.partial.split_off(end + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        for line in complete.split(|&b| b == b'\n') {
            self.apply(line, main);
        }
        Ok(())
    }

    /// Takes the facts of one jsonl line; lines that are not JSON are skipped.
    fn apply(&mut self, line: &[u8], main: bool) {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        match value["type"].as_str() {
            Some("session") if main => {
                if let Some(id) = value["id"].as_str() {
                    id.clone_into(&mut self.id);
                }
                if let Some(ms) = timestamp_ms(&value["timestamp"]) {
                    self.started_ms = ms;
                }
                if let Some(cwd) = value["cwd"].as_str() {
                    cwd.clone_into(&mut self.cwd);
                }
                self.set_title(&value["title"]);
            }
            Some("title" | "title_change") if main => self.set_title(&value["title"]),
            Some("model_change") if main => {
                if let Some(model) = value["model"].as_str() {
                    model.clone_into(&mut self.model);
                }
            }
            Some("message") => self.apply_message(&value["message"], main),
            Some("custom_message")
                if main && value["customType"].as_str() == Some(TASK_MESSAGE) =>
            {
                if let Some(text) = value["content"].as_str() {
                    self.add_markers(text);
                }
            }
            _ => {}
        }
    }

    /// Takes a non-empty title. omp prefixes titles with icon-font glyphs (private-use code
    /// points) that no UI font draws; they are dropped.
    fn set_title(&mut self, title: &Value) {
        let Some(title) = title.as_str() else { return };
        let clean: String = title.chars().filter(|&c| !is_private_use(c)).collect();
        let clean = clean.trim();
        if !clean.is_empty() {
            clean.clone_into(&mut self.title);
        }
    }

    fn apply_message(&mut self, message: &Value, main: bool) {
        match message["role"].as_str() {
            Some("assistant") => {
                self.tokens += message["usage"]["totalTokens"].as_u64().unwrap_or(0);
            }
            // A pasted pick prompt (user) or a `claim` result (toolResult) carries the marker.
            Some("user" | "toolResult") if main => {
                let Some(parts) = message["content"].as_array() else {
                    return;
                };
                for text in parts.iter().filter_map(|p| p["text"].as_str()) {
                    self.add_markers(text);
                }
            }
            _ => {}
        }
    }

    fn add_markers(&mut self, text: &str) {
        for token in markers_in(text) {
            // An agent that echoes the same marker twice in a row adds nothing.
            if self.markers.last() != Some(&token) {
                self.markers.push(token);
            }
        }
    }
}

/// Whether `c` is in a Unicode private-use area (icon fonts put their glyphs there).
fn is_private_use(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}')
}

/// Claim tokens in `text`: `schwitzer-claim: ` and 32 hex digits, not followed by another.
fn markers_in(text: &str) -> impl Iterator<Item = ClaimToken> + '_ {
    text.match_indices(MARKER).filter_map(|(at, _)| {
        let rest = &text[at + MARKER.len()..];
        let hex = rest.get(..MARKER_HEX)?;
        if rest[MARKER_HEX..].starts_with(|c: char| c.is_ascii_hexdigit()) {
            return None;
        }
        ClaimToken::from_str(hex).ok()
    })
}

/// Incremental reader of sessions directories.
#[derive(Default)]
pub struct Scanner {
    files: HashMap<PathBuf, FileState>,
}

impl Scanner {
    /// The sessions under `dirs` that were written within ten minutes of `now_ms`, newest
    /// first. A missing directory has none.
    pub fn scan(&mut self, dirs: &[SessionsDir], now_ms: u64) -> Vec<LocalSession> {
        let cutoff = now_ms.saturating_sub(u64::try_from(ACTIVE_WINDOW.as_millis()).unwrap_or(0));
        let mut seen = HashSet::new();
        let mut sessions = Vec::new();
        for dir in dirs {
            self.scan_dir(dir, cutoff, &mut seen, &mut sessions);
        }
        self.files.retain(|p, _| seen.contains(p));
        sessions.sort_by(|a, b| b.active_ms.cmp(&a.active_ms).then_with(|| a.id.cmp(&b.id)));
        sessions
    }

    /// Adds the sessions of `dir` written since `cutoff` to `sessions`, and the files read to
    /// `seen`.
    fn scan_dir(
        &mut self,
        dir: &SessionsDir,
        cutoff: u64,
        seen: &mut HashSet<PathBuf>,
        sessions: &mut Vec<LocalSession>,
    ) {
        for path in session_files(&dir.path) {
            let subagents = jsonl_files(&path.with_extension(""));
            let main_mtime = mtime_ms(&path);
            let active_ms = subagents
                .iter()
                .filter_map(|p| mtime_ms(p))
                .chain(main_mtime)
                .max();
            let (Some(active_ms), Some(_)) = (active_ms, main_mtime) else {
                continue;
            };
            if active_ms < cutoff {
                continue;
            }
            self.refresh(&path, true);
            seen.insert(path.clone());
            let mut tokens = self.files[&path].tokens;
            for sub in subagents {
                self.refresh(&sub, false);
                tokens += self.files[&sub].tokens;
                seen.insert(sub);
            }
            sessions.push(session(
                &path,
                &self.files[&path],
                active_ms,
                tokens,
                dir.distro.as_deref(),
            ));
        }
    }

    /// Reads the new lines of `path`; a file that cannot be read keeps what it had.
    fn refresh(&mut self, path: &Path, main: bool) {
        let state = self.files.entry(path.to_owned()).or_default();
        if let Err(e) = state.update(path, main) {
            tracing::debug!(path = %path.display(), "session file unreadable: {e}");
        }
    }
}

/// The session `state` describes; `distro` is the WSL distro whose omp wrote it.
fn session(
    path: &Path,
    state: &FileState,
    active_ms: u64,
    tokens: u64,
    distro: Option<&str>,
) -> LocalSession {
    let id = if state.id.is_empty() {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        state.id.clone()
    };
    LocalSession {
        id,
        title: state.title.clone(),
        cwd: match distro {
            Some(d) => crate::wsl::to_windows(d, &state.cwd)
                .map(PathBuf::from)
                .unwrap_or_default(),
            None => PathBuf::from(&state.cwd),
        },
        project: project_of(&state.cwd),
        model: state.model.clone(),
        started_ms: state.started_ms,
        active_ms,
        tokens,
        markers: state.markers.clone(),
    }
}

/// The last component of a working directory.
fn project_of(cwd: &str) -> String {
    cwd.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// `<dir>/*/*.jsonl`.
fn session_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(projects) = fs::read_dir(dir) else {
        return Vec::new();
    };
    projects
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .flat_map(|p| jsonl_files(&p))
        .collect()
}

/// The `*.jsonl` files directly in `dir`; none when it does not exist.
fn jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl") && p.is_file())
        .collect()
}

/// Last write of `path`, ms since the epoch.
fn mtime_ms(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let since = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}

/// Milliseconds since the epoch of an ISO 8601 string or a number of milliseconds.
fn timestamp_ms(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => iso_ms(s),
        _ => None,
    }
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fff][Z|±HH:MM]` (a space may replace the `T`); no zone reads as
/// UTC. Times before the epoch are `None`.
fn iso_ms(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't' | b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let field = |from: usize, to: usize| -> Option<i64> {
        let digits = s.get(from..to)?;
        digits
            .bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| digits.parse().ok())?
    };
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut rest = &s[19..];
    let mut millis = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        // Milliseconds: the first three digits, zero-padded.
        let mut scale = 100;
        for d in fraction.bytes().take(digits.min(3)) {
            millis += i64::from(d - b'0') * scale;
            scale /= 10;
        }
        rest = &fraction[digits..];
    }
    let offset_minutes = match rest {
        "" | "Z" | "z" => 0,
        zone => {
            let sign = match zone.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let digits: String = zone[1..].chars().filter(|&c| c != ':').collect();
            if digits.len() != 4 || !digits.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let (h, m): (i64, i64) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
            sign * (h * 60 + m)
        }
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
        - offset_minutes * 60;
    u64::try_from(seconds * 1000 + millis).ok()
}

/// Days from 1970-01-01 to the proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    fn fixture(name: &str) -> String {
        fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap()
    }

    /// A sessions directory with the fixture session `s1` of project `-proj` and a subagent.
    struct Dir {
        root: tempfile::TempDir,
    }

    impl Dir {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let project = root.path().join("-proj");
            fs::create_dir_all(project.join("s1")).unwrap();
            fs::write(project.join("s1.jsonl"), fixture("session.jsonl")).unwrap();
            fs::write(project.join("s1/sub.jsonl"), fixture("subagent.jsonl")).unwrap();
            Self { root }
        }

        fn session_file(&self) -> PathBuf {
            self.root.path().join("-proj/s1.jsonl")
        }

        fn scan(&self, scanner: &mut Scanner) -> Vec<LocalSession> {
            scanner.scan(&self.dirs(None), schwitzer_proto::now_ms())
        }

        fn dirs(&self, distro: Option<&str>) -> Vec<SessionsDir> {
            vec![SessionsDir {
                path: self.root.path().to_owned(),
                distro: distro.map(str::to_owned),
            }]
        }
    }

    fn token(hex: &str) -> ClaimToken {
        hex.parse().unwrap()
    }

    #[test]
    fn a_session_is_read_from_its_file() {
        let dir = Dir::new();
        let sessions = dir.scan(&mut Scanner::default());
        let [s] = sessions.as_slice() else {
            panic!("{sessions:?}");
        };
        assert_eq!(s.id, "01a14ba1-e453-72dd-bd68-19fe59ca8bd1");
        assert_eq!(s.project, "griasdi");
        assert_eq!(s.model, "anthropic/claude-sonnet-5-5");
        assert_eq!(s.started_ms, 1_792_270_263_379);
    }

    #[test]
    fn tokens_add_up_across_the_session_and_its_subagents() {
        let sessions = Dir::new().scan(&mut Scanner::default());
        // 1000 + 234 of the session, 500 + 66 of the subagent.
        assert_eq!(sessions[0].tokens, 1800);
    }

    #[test]
    fn the_latest_title_wins_and_subagents_do_not_name_the_session() {
        let sessions = Dir::new().scan(&mut Scanner::default());
        assert_eq!(sessions[0].title, "Second title");
        assert_eq!(sessions[0].project, "griasdi");
        assert_eq!(sessions[0].model, "anthropic/claude-sonnet-5-5");
    }

    #[test]
    fn markers_come_from_tool_results_in_order() {
        let sessions = Dir::new().scan(&mut Scanner::default());
        // The 31-digit marker is no marker; the subagent's marker does not count.
        assert_eq!(
            sessions[0].markers,
            [
                token("0123456789abcdef0123456789abcdef"),
                token("fedcba9876543210fedcba9876543210"),
            ]
        );
    }

    #[test]
    fn markers_need_exactly_32_hex_digits() {
        let found = |text: &str| markers_in(text).count();
        assert_eq!(
            found("schwitzer-claim: 0123456789abcdef0123456789abcdef"),
            1
        );
        assert_eq!(
            found("a schwitzer-claim: 0123456789abcdef0123456789abcdef\n"),
            1
        );
        assert_eq!(
            found("schwitzer-claim: 0123456789abcdef0123456789abcdef0"),
            0
        );
        assert_eq!(found("schwitzer-claim: 0123456789abcdef0123456789abcde"), 0);
        assert_eq!(found("schwitzer-claim: <claim token hex>"), 0);
        assert_eq!(
            found("schwitzer-claim: ééééééééééééééééééééééééééééééééé"),
            0
        );
    }

    #[test]
    fn appended_lines_update_the_totals() {
        let dir = Dir::new();
        let mut scanner = Scanner::default();
        assert_eq!(dir.scan(&mut scanner)[0].tokens, 1800);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(dir.session_file())
            .unwrap();
        file.write_all(fixture("append.jsonl").as_bytes()).unwrap();
        let s = &dir.scan(&mut scanner)[0];
        assert_eq!(s.tokens, 1900);
        assert_eq!(s.title, "Third title");
        // Nothing was read twice.
        assert_eq!(s.markers.len(), 2);
    }

    #[test]
    fn a_line_is_read_once_it_is_complete() {
        let dir = Dir::new();
        let mut scanner = Scanner::default();
        dir.scan(&mut scanner);
        let line = fixture("append.jsonl").lines().next().unwrap().to_owned();
        let (head, tail) = line.split_at(30);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(dir.session_file())
            .unwrap();
        file.write_all(head.as_bytes()).unwrap();
        assert_eq!(dir.scan(&mut scanner)[0].tokens, 1800);
        file.write_all(tail.as_bytes()).unwrap();
        file.write_all(b"\n").unwrap();
        assert_eq!(dir.scan(&mut scanner)[0].tokens, 1900);
    }

    #[test]
    fn a_file_that_shrank_is_read_again_from_the_start() {
        let dir = Dir::new();
        let mut scanner = Scanner::default();
        dir.scan(&mut scanner);
        let first: String = fixture("session.jsonl")
            .lines()
            .take(5)
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(dir.session_file(), first).unwrap();
        let s = &dir.scan(&mut scanner)[0];
        assert_eq!(s.tokens, 1566);
        assert_eq!(s.title, "First title");
        assert!(s.markers.is_empty());
    }

    #[test]
    fn a_session_is_active_for_ten_minutes_after_its_last_write() {
        let dir = Dir::new();
        let mut scanner = Scanner::default();
        let now = schwitzer_proto::now_ms();
        assert_eq!(scanner.scan(&dir.dirs(None), now + 9 * 60_000).len(), 1);
        assert!(scanner.scan(&dir.dirs(None), now + 11 * 60_000).is_empty());
        assert!(scanner.files.is_empty());
    }

    #[test]
    fn a_busy_subagent_keeps_an_idle_session_active() {
        let dir = Dir::new();
        let past = SystemTime::now() - Duration::from_secs(3600);
        File::options()
            .write(true)
            .open(dir.session_file())
            .unwrap()
            .set_modified(past)
            .unwrap();
        let s = dir.scan(&mut Scanner::default());
        assert_eq!(s.len(), 1);
        assert!(s[0].active_ms > mtime_ms(&dir.session_file()).unwrap());
    }

    #[test]
    fn a_missing_directory_has_no_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let missing = [SessionsDir {
            path: dir.path().join("none"),
            distro: None,
        }];
        let sessions = Scanner::default().scan(&missing, schwitzer_proto::now_ms());
        assert!(sessions.is_empty());
    }

    #[test]
    fn wsl_sessions_get_windows_working_directories() {
        let local = Dir::new();
        let wsl = Dir::new();
        let dirs = [local.dirs(None), wsl.dirs(Some("Ubuntu"))].concat();
        let mut scanner = Scanner::default();
        for _ in 0..2 {
            let sessions = scanner.scan(&dirs, schwitzer_proto::now_ms());
            let mut cwds: Vec<&str> = sessions.iter().map(|s| s.cwd.to_str().unwrap()).collect();
            cwds.sort_unstable();
            assert_eq!(
                cwds,
                [
                    "/home/felix/code/griasdi",
                    r"\\wsl.localhost\Ubuntu\home\felix\code\griasdi",
                ]
            );
            // Each directory's files keep their own state: nothing is lost or read twice.
            assert!(sessions.iter().all(|s| s.tokens == 1800), "{sessions:?}");
        }
    }

    #[test]
    fn iso_timestamps_and_numbers_become_milliseconds() {
        let ms = iso_ms;
        assert_eq!(ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(ms("2026-10-17T20:51:03.379Z"), Some(1_792_270_263_379));
        assert_eq!(ms("2026-10-17T20:51:03Z"), Some(1_792_270_263_000));
        assert_eq!(ms("2026-10-17T20:51:03.37Z"), Some(1_792_270_263_370));
        assert_eq!(ms("2026-10-17T20:51:03.379123Z"), Some(1_792_270_263_379));
        assert_eq!(ms("2026-10-17T22:51:03.379+02:00"), Some(1_792_270_263_379));
        assert_eq!(ms("2026-10-17T20:51:03.379"), Some(1_792_270_263_379));
        assert_eq!(ms("2024-02-29T00:00:00Z"), Some(1_709_164_800_000));
        assert_eq!(ms("1969-12-31T23:59:59Z"), None);
        assert_eq!(ms("2026-13-17T20:51:03Z"), None);
        assert_eq!(ms("yesterday"), None);
        assert_eq!(ms("2026-10-17T20:51:03.Z"), None);
        assert_eq!(
            timestamp_ms(&Value::from(1_792_270_263_379_u64)),
            Some(1_792_270_263_379)
        );
        assert_eq!(timestamp_ms(&Value::Null), None);
    }

    #[test]
    fn the_project_is_the_last_directory_of_the_cwd() {
        assert_eq!(project_of("/home/felix/code/griasdi"), "griasdi");
        assert_eq!(project_of("/home/felix/code/griasdi/"), "griasdi");
        assert_eq!(project_of("C:\\work\\app"), "app");
        assert_eq!(project_of(""), "");
    }

    #[test]
    fn a_pasted_prompt_links_like_a_claim_result() {
        let mut state = FileState::default();
        let line = br#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"Work on #3\nschwitzer-claim: 0123456789abcdef0123456789abcdef"}]}}"#;
        state.apply(line, true);
        assert_eq!(state.markers, [token("0123456789abcdef0123456789abcdef")]);
        // Subagent prompts do not link the session.
        let mut sub = FileState::default();
        sub.apply(line, false);
        assert!(sub.markers.is_empty());
    }

    #[test]
    fn a_plugin_task_message_links_the_session() {
        let line = br#"{"type":"custom_message","customType":"schwitzer-task","content":"Build #3\nschwitzer-claim: 0123456789abcdef0123456789abcdef","display":true}"#;
        let mut state = FileState::default();
        state.apply(line, true);
        assert_eq!(state.markers, [token("0123456789abcdef0123456789abcdef")]);
        // Other plugin messages do not link the session.
        let other = String::from_utf8_lossy(line).replace("schwitzer-task", "schwitzer-projects");
        let mut state = FileState::default();
        state.apply(other.as_bytes(), true);
        assert!(state.markers.is_empty());
        // Neither does a subagent's.
        let mut sub = FileState::default();
        sub.apply(line, false);
        assert!(sub.markers.is_empty());
    }

    #[test]
    fn icon_glyphs_are_dropped_from_titles() {
        let mut state = FileState::default();
        state.apply(
            "{\"type\":\"title\",\"title\":\"\u{f0756} DESIGN: Redesign\"}".as_bytes(),
            true,
        );
        assert_eq!(state.title, "DESIGN: Redesign");
        // A title of glyphs only keeps the previous one.
        state.apply(
            "{\"type\":\"title\",\"title\":\"\u{f0756}\"}".as_bytes(),
            true,
        );
        assert_eq!(state.title, "DESIGN: Redesign");
    }
}
