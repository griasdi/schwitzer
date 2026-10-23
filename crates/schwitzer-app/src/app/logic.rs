//! What the views and `update` decide without touching iced: how tasks are grouped, which
//! decisions are legal, which local sessions a room sees, and how numbers are shown.

use std::path::{Path, PathBuf};

use schwitzer_proto::{
    ClaimToken, Decision, Member, MemberId, RoomId, RoomState, SessionInfo, Stage, Task, Work, caps,
};
use serde::{Deserialize, Serialize};

use crate::mcp::McpStatus;
use crate::omp_plugin::Coverage;
use crate::sessions::LocalSession;
use crate::theme::Dot;
use crate::update::UpdateState;
use crate::{settings, wsl};

/// A local session counts as idle when its files were last written this long ago.
pub const IDLE_AFTER_MS: u64 = 2 * 60 * 1000;

/// The sections of the task list, in display order: what someone works on (planning or
/// building), what can be picked, what waits for or in planning, what waits for a decision,
/// the ideas, what is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Group {
    InProgress,
    Pickable,
    Planning,
    Proposals,
    Ideas,
    Closed,
}

impl Group {
    pub const ALL: [Self; 6] = [
        Self::InProgress,
        Self::Pickable,
        Self::Planning,
        Self::Proposals,
        Self::Ideas,
        Self::Closed,
    ];

    pub fn of(task: &Task) -> Self {
        match (task.stage, &task.claim) {
            (Stage::Idea, _) => Self::Ideas,
            (Stage::Proposed, _) => Self::Proposals,
            (Stage::Accepted | Stage::Ready, Some(_)) => Self::InProgress,
            (Stage::Ready, None) => Self::Pickable,
            (Stage::Planned, None) if task.has_plan() => Self::Pickable,
            (Stage::Accepted | Stage::Planned, _) => Self::Planning,
            (Stage::Done | Stage::Rejected, _) => Self::Closed,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::InProgress => "In progress",
            Self::Pickable => "Ready to pick",
            Self::Planning => "Planning",
            Self::Proposals => "Proposals",
            Self::Ideas => "Ideas",
            Self::Closed => "Done",
        }
    }

    /// A stable widget id fragment.
    pub fn slug(self) -> &'static str {
        match self {
            Self::InProgress => "in-progress",
            Self::Pickable => "pickable",
            Self::Planning => "planning",
            Self::Proposals => "proposals",
            Self::Ideas => "ideas",
            Self::Closed => "closed",
        }
    }
}

/// Whether the task has an approved plan nobody builds yet: picking it claims the build for
/// the picker.
pub fn pickable(task: &Task) -> bool {
    task.stage == Stage::Ready && task.claim.is_none()
}

/// Whether the task has a finished plan that waits for a human's approval.
pub fn approvable(task: &Task) -> bool {
    task.stage == Stage::Planned && task.has_plan()
}

/// How long after an approval the list waits for the omp session that wrote the plan to
/// claim the build before it offers Pick.
pub const HANDOFF_MS: u64 = 6_000;

/// The tasks whose plan was approved between `old` and `new`: `Planned` in `old`, approved
/// and unclaimed in `new`.
pub fn approved_between(old: &RoomState, new: &RoomState) -> Vec<u64> {
    new.tasks
        .iter()
        .filter(|t| pickable(t))
        .filter(|t| {
            old.tasks
                .iter()
                .any(|o| o.number == t.number && o.stage == Stage::Planned)
        })
        .map(|t| t.number)
        .collect()
}

/// The decisions that wait for a human in `state`, over the projects in `projects` (those this
/// member linked a repo to): proposals to accept and finished plans to approve.
pub fn waiting_decisions(state: &RoomState, projects: &[u64]) -> usize {
    state
        .tasks
        .iter()
        .filter(|t| projects.contains(&t.project))
        .filter(|t| t.stage == Stage::Proposed || approvable(t))
        .count()
}

/// The tasks `member`'s agents handed in between `old` and `new`: a plan claim of `member` in
/// `old` that is `Planned` in `new` (`Work::Plan`), a build claim of `member` in `old` that is
/// `Done` in `new` (`Work::Build`). Ordered by task number.
pub fn hand_ins<'a>(
    old: &RoomState,
    new: &'a RoomState,
    member: &MemberId,
) -> Vec<(Work, &'a Task)> {
    new.tasks
        .iter()
        .filter_map(|t| {
            let before = old.tasks.iter().find(|o| o.number == t.number)?;
            let claim = before.claim.as_ref().filter(|c| c.member == *member)?;
            match (claim.work, t.stage) {
                (Work::Plan, Stage::Planned) => Some((Work::Plan, t)),
                (Work::Build, Stage::Done) => Some((Work::Build, t)),
                _ => None,
            }
        })
        .collect()
}

/// Whether `task`, seen approved at `approved_ms` (local time), still waits for its build
/// claim within [`HANDOFF_MS`]: the app shows Approve busy instead of Pick meanwhile.
pub fn handing_off(task: &Task, approved_ms: Option<u64>, now_ms: u64) -> bool {
    pickable(task) && approved_ms.is_some_and(|at| now_ms.saturating_sub(at) < HANDOFF_MS)
}

/// How long a claim may go without an omp session that carries it before its slab says so:
/// time to paste a pick prompt and for the agent's first write.
pub const CLAIM_GRACE_MS: u64 = 5 * 60 * 1000;

/// Why a claimed task shows no session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingSession {
    /// The claimer is online, and none of their sessions carries the claim.
    Gone,
    /// The claimer is offline: their app shares no sessions, so nobody can tell.
    Unknown,
}

/// Whether a claim older than [`CLAIM_GRACE_MS`] lacks an omp session that carries it. Your
/// own claim counts against every local session, wherever it runs, by its claim marker.
/// Someone else's counts against the sessions they share linked to the task; when they are
/// offline their app shares nothing, so the answer is [`MissingSession::Unknown`].
pub fn missing_session(
    state: &RoomState,
    task: &Task,
    me: MemberId,
    local: &[LocalSession],
    now_ms: u64,
) -> Option<MissingSession> {
    let claim = task.claim.as_ref()?;
    // A `since_ms` ahead of the local clock saturates to 0: no hint.
    if now_ms.saturating_sub(claim.since_ms) < CLAIM_GRACE_MS {
        return None;
    }
    if claim.member == me {
        let carried = local.iter().any(|s| s.markers.contains(&claim.token));
        return (!carried).then_some(MissingSession::Gone);
    }
    // Only the claimer's sessions count: an earlier claimer's stale link must not hide it.
    let carried = state
        .sessions
        .iter()
        .any(|(m, list)| *m == claim.member && list.iter().any(|s| s.task == Some(task.number)));
    if carried {
        return None;
    }
    let online = state
        .members
        .iter()
        .any(|m| m.id == claim.member && m.online);
    Some(if online {
        MissingSession::Gone
    } else {
        MissingSession::Unknown
    })
}

/// The tasks of `project` in `group`, high priority first, then by number.
pub fn tasks_in(state: &RoomState, project: u64, group: Group) -> Vec<&Task> {
    let mut tasks: Vec<&Task> = state
        .tasks
        .iter()
        .filter(|t| t.project == project && Group::of(t) == group)
        .collect();
    tasks.sort_by_key(|t| (t.priority, t.number));
    tasks
}

/// At most this many folders are suggested for one project.
pub const MAX_SUGGESTIONS: usize = 8;
/// A name shorter than this (letters and digits) never matches as part of another.
const MIN_PART: usize = 3;

/// The folders of `dirs` whose name matches `project` (letters and digits only, ignoring case):
/// equal names first, then names that contain the other, without those already `linked`; at
/// most [`MAX_SUGGESTIONS`].
pub fn suggested_repos<'a>(
    project: &str,
    dirs: &'a [PathBuf],
    linked: &[PathBuf],
) -> Vec<&'a PathBuf> {
    let key = name_key(project);
    if key.is_empty() {
        return Vec::new();
    }
    let part =
        |whole: &str, piece: &str| whole.contains(piece) && piece.chars().count() >= MIN_PART;
    let mut matches: Vec<(u8, &PathBuf)> = dirs
        .iter()
        .filter(|dir| !linked.contains(dir))
        .filter_map(|dir| {
            let d = name_key(&dir.file_name()?.to_string_lossy());
            let rank = if d == key {
                0
            } else if part(&d, &key) || part(&key, &d) {
                1
            } else {
                return None;
            };
            Some((rank, dir))
        })
        .collect();
    matches.sort_by_key(|(rank, _)| *rank);
    matches.truncate(MAX_SUGGESTIONS);
    matches.into_iter().map(|(_, dir)| dir).collect()
}

/// A name reduced to its letters and digits, lowercased: "Schwitzer App" and `schwitzer-app`
/// give the same key.
fn name_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The decisions the server accepts for `task` right now (`store::next_stage` of the server), in button order.
pub fn legal_decisions(task: &Task) -> Vec<Decision> {
    match task.stage {
        Stage::Idea => vec![Decision::Reject],
        Stage::Proposed => vec![Decision::Accept, Decision::Reject],
        Stage::Accepted if !task.has_plan() => vec![Decision::Reject],
        Stage::Accepted => vec![Decision::MarkPlanned, Decision::Reject],
        Stage::Planned if !task.has_plan() => vec![Decision::Replan, Decision::Reject],
        Stage::Planned => vec![Decision::Approve, Decision::Replan, Decision::Reject],
        Stage::Ready if task.claim.is_none() => vec![Decision::Replan, Decision::MarkDone],
        Stage::Ready => vec![Decision::MarkDone],
        Stage::Done | Stage::Rejected => vec![Decision::Reopen],
    }
}

/// Whether `EditTask` is allowed for the task.
pub fn editable(task: &Task) -> bool {
    !matches!(task.stage, Stage::Done | Stage::Rejected)
}

/// "waits for #12, #14" for the slab.
pub fn waits_label(open: &[u64]) -> String {
    let numbers: Vec<String> = open.iter().map(|n| format!("#{n}")).collect();
    format!("waits for {}", numbers.join(", "))
}

/// The label of a stage as shown next to a task.
pub fn stage_label(task: &Task) -> &'static str {
    match (task.stage, task.claim.as_ref().map(|c| c.work)) {
        (Stage::Idea, _) => "Idea",
        (Stage::Proposed, _) => "Proposed",
        (Stage::Accepted, Some(Work::Plan)) => "Being planned",
        (Stage::Accepted, _) => "Accepted",
        (Stage::Planned, _) if !task.has_plan() => "Planned",
        (Stage::Planned, _) => "Plan ready",
        (Stage::Ready, Some(_)) => "Building",
        (Stage::Ready, None) => "Approved",
        (Stage::Done, _) => "Done",
        (Stage::Rejected, _) => "Rejected",
    }
}

pub fn work_label(work: Work) -> &'static str {
    match work {
        Work::Plan => "planning",
        Work::Build => "building",
    }
}

/// The name of a member, or the start of the id when the member is unknown.
pub fn member_name(members: &[Member], id: &MemberId) -> String {
    members
        .iter()
        .find(|m| &m.id == id)
        .map_or_else(|| id.short(), |m| m.name.clone())
}

/// The room's name, or `Room <short id>` before its server sent it.
pub fn room_label(id: RoomId, state: Option<&RoomState>) -> String {
    state.map_or_else(|| format!("Room {}", id.short()), |s| s.name.clone())
}

/// Which of the six tag colours a member wears: fixed per member, the same for everyone.
pub fn member_hue(id: &MemberId) -> usize {
    usize::from(id.0[0] ^ id.0[31]) % 6
}

/// A byte count for people: `812 B`, `12.3 KiB`, `4.1 MiB`.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 3] = ["KiB", "MiB", "GiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// What to tell `me` about the change of a room's debug session from `old` to `new`: someone
/// else started one, or the one `me` was in ended (`true`).
pub fn debug_change(old: &RoomState, new: &RoomState, me: &MemberId) -> Option<(String, bool)> {
    match (&old.debug, &new.debug) {
        (None, Some(s)) if s.master != *me => Some((
            format!(
                "{} started a debug session in {}",
                member_name(&new.members, &s.master),
                new.name
            ),
            false,
        )),
        (Some(s), None) if s.members.contains(me) => {
            Some((format!("The debug session in {} ended", new.name), true))
        }
        _ => None,
    }
}

/// The members of a room as the member list shows them: online first, then by name
/// regardless of case, ties by id.
pub fn members_by_presence(members: &[Member]) -> Vec<&Member> {
    let mut list: Vec<&Member> = members.iter().collect();
    list.sort_by_cached_key(|m| (!m.online, m.name.to_lowercase(), m.id.0));
    list
}

/// The sessions of `local` the room sees: those whose working directory lies in a repo linked
/// to one of the room's projects; each names its projects, see `SessionInfo::projects`. A
/// session that holds a claim of `me` on a task of the room (its latest such marker) is linked
/// to that task.
pub fn shared_sessions(
    local: &[LocalSession],
    room: &RoomState,
    me: MemberId,
    repos: &[(u64, &[PathBuf])],
) -> Vec<SessionInfo> {
    let mut shared: Vec<SessionInfo> = local
        .iter()
        .filter_map(|session| {
            let mut projects: Vec<u64> = repos
                .iter()
                .filter(|(_, paths)| in_repos(&session.cwd, paths))
                .map(|(project, _)| *project)
                .collect();
            if projects.is_empty() {
                return None;
            }
            let task = session.markers.iter().rev().find_map(|marker| {
                room.tasks.iter().find(|t| {
                    t.claim
                        .as_ref()
                        .is_some_and(|c| &c.token == marker && c.member == me)
                })
            });
            if let Some(task) = task {
                projects = vec![task.project];
            }
            projects.truncate(caps::SESSION_PROJECTS);
            Some(SessionInfo {
                id: session.id.clone(),
                title: caps::cut(&session.title, caps::SESSION_TITLE).to_owned(),
                project: session.project.clone(),
                model: session.model.clone(),
                started_ms: session.started_ms,
                active_ms: session.active_ms,
                tokens: session.tokens,
                task: task.map(|t| t.number),
                projects,
            })
        })
        .collect();
    shared.sort_by(|a, b| a.id.cmp(&b.id));
    shared.truncate(caps::SESSIONS_PER_PUBLISH);
    shared
}

/// Whether `cwd` is one of `repos` or lies below one, compared by path components.
fn in_repos(cwd: &Path, repos: &[PathBuf]) -> bool {
    !cwd.as_os_str().is_empty() && repos.iter().any(|repo| cwd.starts_with(repo))
}

/// What the picker pastes into their agent: the task, the claim marker that links the agent's
/// session to the task, and the tool calls that finish it.
pub fn pick_prompt(room: &str, task: &Task, token: &ClaimToken) -> String {
    format!(
        "Work on schwitzer task #{number} in room \"{room}\": {title}\n\
         schwitzer-claim: {token}\n\
         The build is claimed for you. Read the task and its approved plan with the tool \
         schwitzer_get_task (room \"{room}\", task {number}), implement the plan, then call \
         schwitzer_finish with a short note, or schwitzer_unclaim if you stop.",
        number = task.number,
        title = task.title,
        token = token.to_hex(),
    )
}

/// The task's title as a level-one heading over its details, the Markdown the copy button of
/// the open task puts on the clipboard; details start at `##`.
pub fn details_markdown(title: &str, body: &str) -> String {
    let body = body.trim_end();
    if body.trim().is_empty() {
        format!("# {}", title.trim())
    } else {
        format!("# {}\n\n{body}", title.trim())
    }
}

/// Whether `url` is an http or https link with something after the scheme: the only
/// links a click in task text opens.
pub fn is_web_link(url: &str) -> bool {
    url.split_once("://").is_some_and(|(scheme, rest)| {
        (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
            && !rest.is_empty()
    })
}

/// The break opportunity [`breakable_path`] puts after a separator: a zero-width space, which
/// allows a line break after it and none before it.
const PATH_BREAK: char = '\u{200B}';

/// `path` with a zero-width space after each separator that sits between two names, so text
/// wraps there: `C:\a\b` and `/a/b` break after their inner separators. A separator run, such as
/// the leading `\\` of `\\server\share`, stays together, and a leading or trailing separator
/// gets none.
pub fn breakable_path(path: &str) -> String {
    let is_separator = |c: char| matches!(c, '/' | '\\');
    let mut out = String::with_capacity(path.len() + path.len() / 4);
    let mut previous: Option<char> = None;
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if is_separator(c)
            && previous.is_some_and(|p| !is_separator(p))
            && chars.peek().is_some_and(|&n| !is_separator(n))
        {
            out.push(PATH_BREAK);
        }
        previous = Some(c);
    }
    out
}

/// `path` as the member knows it: a folder in a WSL distro as its Linux path with the
/// distro, `/home/felix/code (Ubuntu)`; anything else with the home directory as `~`.
pub fn folder_label(path: &Path) -> String {
    match wsl::to_linux(path) {
        Some((distro, linux)) => format!("{linux} ({distro})"),
        None => settings::tilde(path),
    }
}

/// `path` in its canonical spelling (see [`wsl::canonical`]), or the banner text when it is
/// not an absolute path of a folder on this machine.
pub fn existing_folder(path: PathBuf) -> Result<PathBuf, String> {
    let path = wsl::canonical(path);
    if path.is_absolute() && path.is_dir() {
        Ok(path)
    } else {
        Err(format!("Not a folder on this machine: {}", path.display()))
    }
}

/// A token count: `812`, `1.2k`, `12.3k`, `1.2M`.
pub fn tokens(count: u64) -> String {
    let value = count as f64;
    if count < 1_000 {
        count.to_string()
    } else if count < 999_950 {
        format!("{:.1}k", value / 1_000.0)
    } else {
        format!("{:.1}M", value / 1_000_000.0)
    }
}

/// A runtime as `h:mm`.
pub fn runtime(ms: u64) -> String {
    let minutes = ms / 60_000;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// How long ago something was: `<1m`, `5m`, `2h 05m`, `3d`.
pub fn ago(ms: u64) -> String {
    let minutes = ms / 60_000;
    match minutes {
        0 => "<1m".to_owned(),
        1..=59 => format!("{minutes}m"),
        60..=1_439 => format!("{}h {:02}m", minutes / 60, minutes % 60),
        _ => format!("{}d", minutes / 1_440),
    }
}

/// What a check for a new release does next.
#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    /// A release already on offer is at least as new as the announced one.
    Skip,
    /// The announced release is not newer than this build.
    UpToDate,
    /// Look up this release; `None` asks GitHub for its latest.
    Ask(Option<semver::Version>),
}

/// Decides a check from where the updater stands, whether the member asked for GitHub's
/// latest release (`latest`), the release the servers announced and this build's version.
pub fn next_check(
    updater: &UpdateState,
    latest: bool,
    announced: Option<semver::Version>,
    current: &semver::Version,
) -> Check {
    let target = if latest { None } else { announced };
    if let UpdateState::Available { release, .. } = updater
        && !latest
        && target.as_ref().is_some_and(|t| *t <= release.version)
    {
        return Check::Skip;
    }
    if target.as_ref().is_some_and(|version| version <= current) {
        return Check::UpToDate;
    }
    Check::Ask(target)
}

/// One line of the status lamp's card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// What the line is about: "Server", "MCP" or "omp".
    pub name: &'static str,
    pub dot: Dot,
    /// The word beside the status bar's lamp while this line is the worst.
    pub word: &'static str,
    /// The line's detail in the card.
    pub detail: String,
}

/// The status bar's lamp and its word: the worst of `statuses`, the first of equals; "ready"
/// while every one is up.
pub fn overall(statuses: &[Status]) -> (Dot, &'static str) {
    let worst = statuses
        .iter()
        .reduce(|worst, s| if s.dot > worst.dot { s } else { worst });
    match worst {
        Some(s) if s.dot != Dot::Up => (s.dot, s.word),
        _ => (Dot::Up, "ready"),
    }
}

/// The agent port's line; `mcp` is `None` while its server starts, `port` is the port the
/// settings ask for.
pub fn mcp_status(mcp: Option<&McpStatus>, port: u16) -> Status {
    let (dot, word, detail) = match mcp {
        None => (Dot::Unknown, "starting", "Starting…".to_owned()),
        Some(McpStatus::Listening { url }) => (Dot::Up, "ready", url.clone()),
        Some(m @ McpStatus::PortInUse) => (
            Dot::Down,
            "port in use",
            m.problem(port).unwrap_or_default(),
        ),
        Some(m @ McpStatus::Failed(_)) => {
            (Dot::Down, "MCP failed", m.problem(port).unwrap_or_default())
        }
    };
    Status {
        name: "MCP",
        dot,
        word,
        detail,
    }
}

/// The omp plugin's line for the selected project's repos.
pub fn omp_status(coverage: Coverage) -> Status {
    let Coverage {
        repos,
        missing,
        outdated,
    } = coverage;
    let (dot, word, detail) = if repos == 0 {
        (Dot::Unknown, "no repo", "No repo linked".to_owned())
    } else if outdated > 0 {
        let detail = if repos == 1 {
            "Plugin out of date".to_owned()
        } else {
            format!("Plugin out of date in {outdated} of {repos} repos")
        };
        (Dot::Attention, "plugin outdated", detail)
    } else if missing > 0 {
        let detail = if repos == 1 {
            "Plugin missing".to_owned()
        } else {
            format!("Plugin missing in {missing} of {repos} repos")
        };
        (Dot::Attention, "plugin missing", detail)
    } else {
        let detail = if repos == 1 {
            "Plugin current".to_owned()
        } else {
            format!("Plugin current in {repos} repos")
        };
        (Dot::Up, "ready", detail)
    };
    Status {
        name: "omp",
        dot,
        word,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use schwitzer_proto::{Claim, Priority, Project};

    use super::*;

    #[test]
    fn byte_counts_switch_unit_at_1024() {
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(12_595), "12.3 KiB");
        assert_eq!(bytes(4_299_161), "4.1 MiB");
    }

    fn member(byte: u8) -> MemberId {
        MemberId([byte; 32])
    }

    fn status(dot: Dot, word: &'static str) -> Status {
        Status {
            name: "test",
            dot,
            word,
            detail: String::new(),
        }
    }

    #[test]
    fn status_lamp_shows_the_worst_status_first_of_equals() {
        let up = || status(Dot::Up, "ready");
        assert_eq!(overall(&[up(), up(), up()]), (Dot::Up, "ready"));
        assert_eq!(
            overall(&[
                status(Dot::Unknown, "connecting"),
                up(),
                status(Dot::Attention, "plugin missing"),
            ]),
            (Dot::Attention, "plugin missing")
        );
        assert_eq!(
            overall(&[
                up(),
                status(Dot::Down, "port in use"),
                status(Dot::Attention, "plugin missing"),
            ]),
            (Dot::Down, "port in use")
        );
        assert_eq!(
            overall(&[
                status(Dot::Unknown, "connecting"),
                up(),
                status(Dot::Unknown, "no repo"),
            ]),
            (Dot::Unknown, "connecting")
        );
    }

    #[test]
    fn omp_status_wants_a_hand_only_for_a_missing_or_outdated_plugin() {
        let of = |repos, missing, outdated| {
            let s = omp_status(Coverage {
                repos,
                missing,
                outdated,
            });
            (s.dot, s.word, s.detail)
        };
        assert_eq!(
            of(0, 0, 0),
            (Dot::Unknown, "no repo", "No repo linked".to_owned())
        );
        assert_eq!(
            of(1, 1, 0),
            (
                Dot::Attention,
                "plugin missing",
                "Plugin missing".to_owned()
            )
        );
        assert_eq!(
            of(2, 1, 1),
            (
                Dot::Attention,
                "plugin outdated",
                "Plugin out of date in 1 of 2 repos".to_owned()
            )
        );
        assert_eq!(
            of(2, 0, 0),
            (Dot::Up, "ready", "Plugin current in 2 repos".to_owned())
        );
    }

    #[test]
    fn details_markdown_puts_the_title_over_the_details() {
        assert_eq!(
            details_markdown("Fix it", "## Why\n\nBecause.\n"),
            "# Fix it\n\n## Why\n\nBecause."
        );
        assert_eq!(details_markdown("Fix it", "  \n"), "# Fix it");
    }

    #[test]
    fn member_list_puts_online_first_then_names_without_case() {
        let m = |byte, name: &str, online| Member {
            id: member(byte),
            name: name.to_owned(),
            online,
        };
        let members = [
            m(3, "carl", false),
            m(1, "Bob", true),
            m(2, "alice", true),
            m(4, "Anna", false),
        ];
        let names: Vec<&str> = members_by_presence(&members)
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["alice", "Bob", "Anna", "carl"]);
    }

    #[test]
    fn only_web_links_open() {
        assert!(is_web_link("https://example.org/a"));
        assert!(is_web_link("HTTP://example.org"));
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "mailto:a@b.c",
            "https://",
            "docs/plan.md",
            "",
        ] {
            assert!(!is_web_link(url), "{url}");
        }
    }

    fn task(number: u64, stage: Stage, claim: Option<Claim>) -> Task {
        Task {
            number,
            project: 1,
            title: format!("task {number}"),
            body: String::new(),
            plan: String::new(),
            stage,
            priority: Priority::Normal,
            author: member(1),
            created_ms: 0,
            updated_ms: 0,
            claim,
            notes: Vec::new(),
            waits_for: Vec::new(),
        }
    }

    fn claim(work: Work, token: ClaimToken) -> Claim {
        Claim {
            member: member(1),
            agent: "agent".to_owned(),
            work,
            since_ms: 0,
            token,
        }
    }

    fn room(tasks: Vec<Task>) -> RoomState {
        RoomState {
            id: schwitzer_proto::RoomId([0; 32]),
            name: "room".to_owned(),
            projects: vec![Project {
                id: 1,
                name: "app".to_owned(),
            }],
            members: Vec::new(),
            tasks,
            sessions: Vec::new(),
            debug: None,
        }
    }

    fn local(id: &str, cwd: &str, markers: Vec<ClaimToken>) -> LocalSession {
        LocalSession {
            id: id.to_owned(),
            title: "title".to_owned(),
            cwd: PathBuf::from(cwd),
            project: "proj".to_owned(),
            model: "model".to_owned(),
            started_ms: 10,
            active_ms: 20,
            tokens: 30,
            markers,
        }
    }

    #[test]
    fn missing_session_needs_an_old_claim_without_a_session() {
        let token = ClaimToken([7; 16]);
        let now = CLAIM_GRACE_MS;
        let claimed = task(1, Stage::Accepted, Some(claim(Work::Plan, token)));
        let mut state = room(vec![claimed.clone()]);
        assert_eq!(
            missing_session(&state, &claimed, member(1), &[], now - 1),
            None
        );
        let free = task(2, Stage::Accepted, None);
        assert_eq!(missing_session(&state, &free, member(1), &[], now), None);

        // Own claim: every local session counts, even one outside every repo.
        assert_eq!(
            missing_session(&state, &claimed, member(1), &[], now),
            Some(MissingSession::Gone)
        );
        let carrying = [local("s", "/x", vec![token])];
        assert_eq!(
            missing_session(&state, &claimed, member(1), &carrying, now),
            None
        );

        // Someone else's claim: their shared sessions and presence decide.
        let alice = |online| Member {
            id: member(1),
            name: "alice".to_owned(),
            online,
        };
        state.members = vec![alice(true)];
        assert_eq!(
            missing_session(&state, &claimed, member(2), &[], now),
            Some(MissingSession::Gone)
        );
        let session = SessionInfo {
            id: "s".to_owned(),
            title: "title".to_owned(),
            project: "proj".to_owned(),
            model: "model".to_owned(),
            started_ms: 0,
            active_ms: 0,
            tokens: 0,
            task: Some(1),
            projects: vec![1],
        };
        state.sessions = vec![(member(1), vec![session.clone()])];
        assert_eq!(missing_session(&state, &claimed, member(2), &[], now), None);
        state.sessions = vec![(member(3), vec![session])];
        assert_eq!(
            missing_session(&state, &claimed, member(2), &[], now),
            Some(MissingSession::Gone)
        );
        state.members = vec![alice(false)];
        assert_eq!(
            missing_session(&state, &claimed, member(2), &[], now),
            Some(MissingSession::Unknown)
        );
        state.members.clear();
        assert_eq!(
            missing_session(&state, &claimed, member(2), &[], now),
            Some(MissingSession::Unknown)
        );
    }

    #[test]
    fn groups_follow_stage_claim_and_plan() {
        let token = ClaimToken([1; 16]);
        assert_eq!(Group::of(&task(1, Stage::Idea, None)), Group::Ideas);
        assert_eq!(Group::of(&task(1, Stage::Proposed, None)), Group::Proposals);
        assert_eq!(
            Group::of(&task(1, Stage::Accepted, Some(claim(Work::Plan, token)))),
            Group::InProgress
        );
        assert_eq!(Group::of(&task(1, Stage::Accepted, None)), Group::Planning);
        // A planned task without plan text cannot be approved, so it is not in "Ready to pick".
        let mut planned = task(1, Stage::Planned, None);
        assert_eq!(Group::of(&planned), Group::Planning);
        planned.plan = "Do it".to_owned();
        assert_eq!(Group::of(&planned), Group::Pickable);
        assert!(!pickable(&planned));
        assert!(approvable(&planned));
        assert_eq!(Group::of(&task(1, Stage::Ready, None)), Group::Pickable);
        assert!(pickable(&task(1, Stage::Ready, None)));
        let building = task(1, Stage::Ready, Some(claim(Work::Build, token)));
        assert_eq!(Group::of(&building), Group::InProgress);
        assert!(!pickable(&building));
        assert_eq!(Group::of(&task(1, Stage::Done, None)), Group::Closed);
        assert_eq!(Group::of(&task(1, Stage::Rejected, None)), Group::Closed);
    }

    #[test]
    fn rows_sort_by_priority_then_number() {
        let mut low = task(1, Stage::Proposed, None);
        low.priority = Priority::Low;
        let mut high = task(3, Stage::Proposed, None);
        high.priority = Priority::High;
        let normal = task(2, Stage::Proposed, None);
        let mut elsewhere = task(4, Stage::Proposed, None);
        elsewhere.project = 2;
        let state = room(vec![low, normal, high, elsewhere]);
        let numbers: Vec<u64> = tasks_in(&state, 1, Group::Proposals)
            .iter()
            .map(|t| t.number)
            .collect();
        assert_eq!(numbers, [3, 2, 1]);
    }

    #[test]
    fn suggestions_rank_equal_names_before_parts_and_skip_linked_repos() {
        let dirs: Vec<PathBuf> = [
            "/code/app",
            "/code/ap",
            "/code/other",
            "/code/schwitzer-app",
            "/code/schwitzer-app-old",
            "/code/SchwitzerApp",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(
            suggested_repos("Schwitzer App", &dirs, &[]),
            [&dirs[3], &dirs[5], &dirs[0], &dirs[4]]
        );
        let linked = [PathBuf::from("/code/SchwitzerApp")];
        assert_eq!(
            suggested_repos("Schwitzer App", &dirs, &linked),
            [&dirs[3], &dirs[0], &dirs[4]]
        );
        assert!(suggested_repos("ab", &[PathBuf::from("/code/abc")], &[]).is_empty());
        assert!(suggested_repos("--", &dirs, &[]).is_empty());
        let many: Vec<PathBuf> = (0..10)
            .map(|i| PathBuf::from(format!("/code/x-demo-{i}")))
            .collect();
        assert_eq!(suggested_repos("demo", &many, &[]).len(), MAX_SUGGESTIONS);
    }

    #[test]
    fn decisions_match_the_server_table() {
        let token = ClaimToken([1; 16]);
        assert_eq!(
            legal_decisions(&task(1, Stage::Idea, None)),
            [Decision::Reject]
        );
        assert_eq!(
            legal_decisions(&task(1, Stage::Proposed, None)),
            [Decision::Accept, Decision::Reject]
        );
        assert_eq!(
            legal_decisions(&task(1, Stage::Accepted, None)),
            [Decision::Reject]
        );
        let mut accepted = task(1, Stage::Accepted, None);
        accepted.plan = "Do it".to_owned();
        assert_eq!(
            legal_decisions(&accepted),
            [Decision::MarkPlanned, Decision::Reject]
        );
        let mut planned = task(1, Stage::Planned, None);
        assert_eq!(
            legal_decisions(&planned),
            [Decision::Replan, Decision::Reject]
        );
        planned.plan = "Do it".to_owned();
        assert_eq!(
            legal_decisions(&planned),
            [Decision::Approve, Decision::Replan, Decision::Reject]
        );
        assert_eq!(
            legal_decisions(&task(1, Stage::Ready, None)),
            [Decision::Replan, Decision::MarkDone]
        );
        assert_eq!(
            legal_decisions(&task(1, Stage::Ready, Some(claim(Work::Build, token)))),
            [Decision::MarkDone]
        );
        assert_eq!(
            legal_decisions(&task(1, Stage::Done, None)),
            [Decision::Reopen]
        );
        assert_eq!(
            legal_decisions(&task(1, Stage::Rejected, None)),
            [Decision::Reopen]
        );
    }

    #[test]
    fn rooms_see_the_sessions_of_their_repos() {
        let first = ClaimToken([1; 16]);
        let second = ClaimToken([2; 16]);
        let stale = ClaimToken([3; 16]);
        let state = room(vec![
            task(1, Stage::Ready, Some(claim(Work::Build, first))),
            task(2, Stage::Ready, Some(claim(Work::Build, second))),
        ]);
        let repos = [PathBuf::from("/code/app")];
        let sessions = [
            local("b", "/code/app/crates/x", vec![first, second, stale]),
            local("a", "/code/app", vec![stale]),
            local("c", "/code/app", vec![second, first]),
            // A sibling directory with the repo's name as prefix is another repository.
            local("d", "/code/app-old", vec![first]),
            local("e", "/home/me", Vec::new()),
        ];
        let shared = shared_sessions(&sessions, &state, member(1), &[(1, &repos[..])]);
        let pairs: Vec<(&str, Option<u64>)> =
            shared.iter().map(|s| (s.id.as_str(), s.task)).collect();
        // `stale` matches no claimed task, so the latest marker of "b" that matches is `second`.
        assert_eq!(pairs, [("a", None), ("b", Some(2)), ("c", Some(1))]);
        assert_eq!(shared[0].tokens, 30);
        // Another member's claim tokens in a local log link nothing, but the session is shared.
        let other = shared_sessions(&sessions, &state, member(2), &[(1, &repos[..])]);
        assert!(other.iter().all(|s| s.task.is_none()));
        assert_eq!(other.len(), 3);
        // A room without repos sees nothing.
        assert!(shared_sessions(&sessions, &state, member(1), &[]).is_empty());
    }

    #[test]
    fn sessions_belong_to_the_projects_of_their_repos() {
        let token = ClaimToken([4; 16]);
        let mut linked = task(4, Stage::Ready, Some(claim(Work::Build, token)));
        linked.project = 2;
        let state = room(vec![linked]);
        let first = [PathBuf::from("/code/app"), PathBuf::from("/code/shared")];
        let second = [PathBuf::from("/code/web"), PathBuf::from("/code/shared")];
        let repos = [(1, &first[..]), (2, &second[..])];
        let sessions = [
            local("a", "/code/app", Vec::new()),
            local("s", "/code/shared", Vec::new()),
            local("w", "/code/web/src", Vec::new()),
            // Linked to a task of project 2, though its repo belongs to project 1 only.
            local("t", "/code/app", vec![token]),
            local("o", "/home/me", Vec::new()),
        ];
        let shared = shared_sessions(&sessions, &state, member(1), &repos);
        let pairs: Vec<(&str, Vec<u64>)> = shared
            .iter()
            .map(|s| (s.id.as_str(), s.projects.clone()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("a", vec![1]),
                ("s", vec![1, 2]),
                ("t", vec![2]),
                ("w", vec![2])
            ]
        );
    }

    #[test]
    fn only_an_approval_starts_the_handoff() {
        let token = ClaimToken([5; 16]);
        let mut planned = task(1, Stage::Planned, None);
        planned.plan = "plan".to_owned();
        let mut planned_3 = planned.clone();
        planned_3.number = 3;
        let old = room(vec![
            planned,
            task(2, Stage::Ready, Some(claim(Work::Build, token))),
            planned_3,
        ]);
        let new = room(vec![
            task(1, Stage::Ready, None),
            task(2, Stage::Ready, None),
            task(3, Stage::Ready, Some(claim(Work::Build, token))),
            task(4, Stage::Ready, None),
        ]);
        assert_eq!(approved_between(&old, &new), [1]);
    }

    #[test]
    fn waiting_decisions_counts_proposals_and_plans_of_linked_projects() {
        let mut planned = task(2, Stage::Planned, None);
        planned.plan = "plan".to_owned();
        let mut other = task(6, Stage::Proposed, None);
        other.project = 2;
        let state = room(vec![
            task(1, Stage::Proposed, None),
            planned,
            task(3, Stage::Planned, None),
            task(4, Stage::Accepted, None),
            task(5, Stage::Idea, None),
            other,
        ]);
        assert_eq!(waiting_decisions(&state, &[1]), 2);
        assert_eq!(waiting_decisions(&state, &[]), 0);
        assert_eq!(waiting_decisions(&state, &[1, 2]), 3);
    }

    #[test]
    fn hand_ins_report_own_plan_and_build_claims_only() {
        let token = ClaimToken([7; 16]);
        let mut foreign = claim(Work::Plan, token);
        foreign.member = member(2);
        let old = room(vec![
            task(1, Stage::Accepted, Some(claim(Work::Plan, token))),
            task(2, Stage::Ready, Some(claim(Work::Build, token))),
            task(3, Stage::Accepted, Some(foreign)),
            task(4, Stage::Accepted, Some(claim(Work::Plan, token))),
        ]);
        let new = room(vec![
            task(1, Stage::Planned, None),
            task(2, Stage::Done, None),
            task(3, Stage::Planned, None),
            task(4, Stage::Accepted, None),
        ]);
        let numbers = |ins: Vec<(Work, &Task)>| -> Vec<(Work, u64)> {
            ins.into_iter().map(|(w, t)| (w, t.number)).collect()
        };
        assert_eq!(
            numbers(hand_ins(&old, &new, &member(1))),
            [(Work::Plan, 1), (Work::Build, 2)]
        );
        assert!(hand_ins(&new, &new, &member(1)).is_empty());
    }

    #[test]
    fn the_handoff_ends_after_its_window_or_a_claim() {
        let ready = task(1, Stage::Ready, None);
        assert!(handing_off(&ready, Some(1000), 1000 + HANDOFF_MS - 1));
        assert!(!handing_off(&ready, Some(1000), 1000 + HANDOFF_MS));
        assert!(!handing_off(&ready, None, 1000));
        let claimed = task(
            1,
            Stage::Ready,
            Some(claim(Work::Build, ClaimToken([6; 16]))),
        );
        assert!(!handing_off(&claimed, Some(1000), 1000));
    }

    #[test]
    fn the_pick_prompt_carries_a_marker_the_scanner_finds() {
        let token = ClaimToken([0xab; 16]);
        let prompt = pick_prompt("room", &task(7, Stage::Ready, None), &token);
        assert!(prompt.contains(&format!("\nschwitzer-claim: {}\n", token.to_hex())));
        assert!(prompt.starts_with("Work on schwitzer task #7"));
    }

    #[test]
    fn numbers_are_compact() {
        assert_eq!(tokens(812), "812");
        assert_eq!(tokens(1234), "1.2k");
        assert_eq!(tokens(12_345), "12.3k");
        assert_eq!(tokens(1_234_567), "1.2M");
        assert_eq!(runtime(0), "0:00");
        assert_eq!(runtime(75 * 60_000), "1:15");
        assert_eq!(ago(30_000), "<1m");
        assert_eq!(ago(5 * 60_000), "5m");
        assert_eq!(ago(125 * 60_000), "2h 05m");
        assert_eq!(ago(3 * 24 * 60 * 60_000), "3d");
    }

    #[test]
    fn paths_break_after_inner_separators_only() {
        let expect = |s: &str| s.replace('|', &PATH_BREAK.to_string());
        assert_eq!(
            breakable_path(r"\\wsl.localhost\Ubuntu\home\felix\code\griasdi"),
            expect(r"\\wsl.localhost\|Ubuntu\|home\|felix\|code\|griasdi"),
        );
        assert_eq!(
            breakable_path(r"C:\Users\felix\code\griasdi-runner"),
            expect(r"C:\|Users\|felix\|code\|griasdi-runner"),
        );
        assert_eq!(
            breakable_path("/tmp/sw/code/demo/"),
            expect("/tmp/|sw/|code/|demo/"),
        );
        assert_eq!(breakable_path("~"), "~");
        assert_eq!(breakable_path(""), "");
    }

    #[test]
    fn a_check_asks_unless_the_target_is_known_not_to_be_news() {
        let v = |s: &str| semver::Version::parse(s).unwrap();
        let release = |version: &str| crate::update::Release {
            version: v(version),
            url: String::new(),
            size: 0,
            sha256: [0; 32],
        };
        let current = v("1.0.0");
        let idle = UpdateState::Idle;
        let offered = UpdateState::Available {
            release: release("1.2.0"),
            error: None,
        };
        // No announcement: GitHub is asked.
        assert_eq!(next_check(&idle, false, None, &current), Check::Ask(None));
        // An announcement above this build is looked up; one at or below it is not.
        assert_eq!(
            next_check(&idle, false, Some(v("1.1.0")), &current),
            Check::Ask(Some(v("1.1.0")))
        );
        assert_eq!(
            next_check(&idle, false, Some(v("1.0.0")), &current),
            Check::UpToDate
        );
        // "Check now" ignores the announcement.
        assert_eq!(
            next_check(&idle, true, Some(v("1.1.0")), &current),
            Check::Ask(None)
        );
        // A release found above the announced one stays on offer, unless the member asks again.
        assert_eq!(
            next_check(&offered, false, Some(v("1.1.0")), &current),
            Check::Skip
        );
        assert_eq!(
            next_check(&offered, true, Some(v("1.1.0")), &current),
            Check::Ask(None)
        );
        assert_eq!(
            next_check(&offered, false, Some(v("1.3.0")), &current),
            Check::Ask(Some(v("1.3.0")))
        );
    }
}
