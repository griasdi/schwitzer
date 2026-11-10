//! What the views and `update` decide without touching iced: how tasks are grouped, which
//! local sessions a room sees, and how numbers are shown.

use std::path::{Path, PathBuf};

use schwitzer_proto::{
    ClaimToken, Member, MemberId, Part, RoomId, RoomState, ServerAddr, SessionInfo, Stage,
    Suggestion, Task, Work, caps,
};
use schwitzer_release::Channel;
use serde::{Deserialize, Serialize};

use crate::mcp::McpStatus;
use crate::omp_plugin::Coverage;
use crate::sessions::{self, LocalSession};
use crate::theme::Dot;
use crate::update::{Lookup, UpdateState};
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
            let before = old.task(t.number)?;
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

/// The tasks of `project` in `group`: Ideas by votes (most first), then high priority first,
/// then by number; every other group by priority, then number.
pub fn tasks_in(state: &RoomState, project: u64, group: Group) -> Vec<&Task> {
    let mut tasks: Vec<&Task> = state
        .tasks
        .iter()
        .filter(|t| t.project == project && Group::of(t) == group)
        .collect();
    if group == Group::Ideas {
        tasks.sort_by_key(|t| (std::cmp::Reverse(t.votes.len()), t.priority, t.number));
    } else {
        tasks.sort_by_key(|t| (t.priority, t.number));
    }
    tasks
}

/// What the task list scrolls to after the open proposal is accepted, as a widget id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnAnchor {
    /// The proposal after it: its top goes where the accepted slab's top was.
    Next(String),
    /// The proposal before it, else the Proposals header: its bottom plus the slab spacing
    /// goes there.
    Above(String),
}

/// The anchor for accepting proposal `number`; `proposals` is the Proposals group in list
/// order (`tasks_in(.., Group::Proposals)`). `None` when `number` is not among them.
pub fn return_anchor(proposals: &[&Task], number: u64) -> Option<ReturnAnchor> {
    let i = proposals.iter().position(|t| t.number == number)?;
    Some(match proposals.get(i + 1) {
        Some(next) => ReturnAnchor::Next(format!("task-{}", next.number)),
        None if i > 0 => ReturnAnchor::Above(format!("task-{}", proposals[i - 1].number)),
        None => ReturnAnchor::Above(format!("group-{}", Group::Proposals.slug())),
    })
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

/// The name of a member, or the start of the id when the member is unknown or never said
/// hello (the server sends an empty name then).
pub fn member_name(members: &[Member], id: &MemberId) -> String {
    members
        .iter()
        .find(|m| &m.id == id)
        .map(|m| m.name.as_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| id.short(), str::to_owned)
}

/// The tooltip of an idea's vote count: "No votes yet", or "Upvoted by alice, bob" in the
/// order of `votes`.
pub fn voters_label(members: &[Member], votes: &[MemberId]) -> String {
    if votes.is_empty() {
        return "No votes yet".to_owned();
    }
    let names: Vec<String> = votes.iter().map(|id| member_name(members, id)).collect();
    format!("Upvoted by {}", names.join(", "))
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
/// else started one, or the one `me` was in ended.
pub fn debug_change(old: &RoomState, new: &RoomState, me: &MemberId) -> Option<String> {
    match (&old.debug, &new.debug) {
        (None, Some(s)) if s.master != *me => Some(format!(
            "{} started a debug session in {}",
            member_name(&new.members, &s.master),
            new.name
        )),
        (Some(s), None) if s.members.contains(me) => {
            Some(format!("The debug session in {} ended", new.name))
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

/// The title and details of a quick idea: the first line and the rest, each trimmed;
/// `None` when nothing but whitespace was typed.
pub fn split_idea(text: &str) -> Option<(&str, &str)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (title, body) = text.split_once('\n').unwrap_or((text, ""));
    Some((title.trim(), body.trim()))
}

/// What the picker pastes into their agent: the task, the claim marker that links the agent's
/// session to the task, and the tool calls that finish it.
pub fn pick_prompt(room: &str, task: &Task, token: &ClaimToken) -> String {
    format!(
        "Work on schwitzer task #{number} in room \"{room}\": {title}\n\
         {marker}\n\
         The build is claimed for you. Read the task and its approved plan with the tool \
         schwitzer_get_task (room \"{room}\", task {number}), implement the plan, then call \
         schwitzer_finish with a short note, or schwitzer_unclaim if you stop.",
        number = task.number,
        title = task.title,
        marker = sessions::marker_line(token),
    )
}

/// What a copied omp command starts for a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmpStart {
    /// `/schwitzer plan <n> now`: claims the accepted task for planning, in plan mode.
    Plan(u64),
    /// `/schwitzer build <n> now`: claims the approved task and builds it.
    Build(u64),
    /// `/schwitzer propose <n>`: writes the idea up into a proposal.
    WriteUp(u64),
    /// `omp --resume <id>`: this member's local session that carries the task's claim.
    Resume(String),
}

/// What a copied omp command starts for `task`, seen by `me`: the plugin command that fits its
/// stage, or for a task claimed by `me` the newest local session carrying the claim. `None`
/// for someone else's claim, a proposal (its command comes with accepting it) and a stage no
/// agent works on. A session id comes from an omp file and goes into a shell, so one with
/// other characters than ASCII letters, digits, `-` and `_` is never offered.
pub fn omp_start(
    state: &RoomState,
    task: &Task,
    me: MemberId,
    local: &[LocalSession],
) -> Option<OmpStart> {
    if let Some(claim) = &task.claim {
        if claim.member != me {
            return None;
        }
        let shell_safe = |id: &str| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        return local
            .iter()
            .filter(|s| s.markers.contains(&claim.token) && shell_safe(&s.id))
            .max_by_key(|s| s.active_ms)
            .map(|s| OmpStart::Resume(s.id.clone()));
    }
    match task.stage {
        Stage::Idea => Some(OmpStart::WriteUp(task.number)),
        Stage::Accepted => Some(OmpStart::Plan(task.number)),
        Stage::Ready if task.open_waits(&state.tasks).is_empty() => {
            Some(OmpStart::Build(task.number))
        }
        _ => None,
    }
}

/// The shell command that starts `start`: `omp --cwd <repo> '<slash command>'`, or
/// `omp --resume <id>`, which needs no repo. A repo in a WSL distro gets its Linux path and
/// POSIX quoting, for the distro's shell; on Windows any other repo is double-quoted for cmd
/// and PowerShell. `None` without a repo, or for a path that is no UTF-8.
pub fn omp_command(start: &OmpStart, repo: Option<&Path>, windows: bool) -> Option<String> {
    let slash = match start {
        OmpStart::Resume(id) => return Some(format!("omp --resume {id}")),
        OmpStart::Plan(n) => format!("/schwitzer plan {n} now"),
        OmpStart::Build(n) => format!("/schwitzer build {n} now"),
        OmpStart::WriteUp(n) => format!("/schwitzer propose {n}"),
    };
    let repo = repo?;
    let (cwd, slash) = if let Some((_, linux)) = wsl::to_linux(repo) {
        (sh_quote(&linux), sh_quote(&slash))
    } else {
        let path = repo.to_str()?;
        if windows {
            (format!("\"{path}\""), format!("\"{slash}\""))
        } else {
            (sh_quote(path), sh_quote(&slash))
        }
    };
    Some(format!("omp --cwd {cwd} {slash}"))
}

/// `s` in single quotes for a POSIX shell.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
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

/// The text of `part` a diff compares: the details as [`details_markdown`], so a new title
/// shows as a changed `#` line, the plan as it is.
pub fn part_text(part: Part, title: &str, text: &str) -> String {
    match part {
        Part::Details => details_markdown(title, text),
        Part::Plan => text.to_owned(),
    }
}

/// Whether the part a suggestion rewrites has a newer version than the one it was written on.
pub fn outdated(task: &Task, s: &Suggestion) -> bool {
    s.version != task.version(s.part)
}

/// The name of `part` in messages, widget ids and tool rows.
pub fn part_name(part: Part) -> &'static str {
    match part {
        Part::Details => "details",
        Part::Plan => "plan",
    }
}

/// The pulldown-cmark options of iced's Markdown parser (`iced_widget::markdown::parse_with`).
const MARKDOWN_OPTIONS: pulldown_cmark::Options =
    pulldown_cmark::Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        .union(pulldown_cmark::Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS)
        .union(pulldown_cmark::Options::ENABLE_TABLES)
        .union(pulldown_cmark::Options::ENABLE_STRIKETHROUGH)
        .union(pulldown_cmark::Options::ENABLE_TASKLISTS);

/// Where a block sits in a top-level list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListPlace {
    /// Which top-level list, counted from 0 in the text.
    pub list: usize,
    /// The number an ordered list shows for this item: the list's start plus the item's index.
    pub number: Option<u64>,
    /// Width of the list's number column in digits, as iced's ordered list computes it.
    pub digits: f32,
}

/// A block of Markdown: a top-level block, or an item of a top-level list.
#[derive(Debug, Clone, PartialEq)]
pub struct MdBlock {
    /// Byte range of the block's source in the text.
    pub range: std::ops::Range<usize>,
    pub list: Option<ListPlace>,
}

/// The blocks of a Markdown text.
#[derive(Debug, Clone, PartialEq)]
pub struct MdBlocks {
    pub blocks: Vec<MdBlock>,
    /// The text's link reference definitions, one per line, appended when a block is parsed alone.
    pub references: String,
}

/// Splits Markdown into the blocks a comment can sit under: top-level blocks (metadata left
/// out) and the items of top-level lists, with nested lists inside their item.
pub fn md_blocks(src: &str) -> MdBlocks {
    use pulldown_cmark::{Event, Tag};

    let parser = pulldown_cmark::Parser::new_ext(src, MARKDOWN_OPTIONS);
    let mut definitions: Vec<_> = parser
        .reference_definitions()
        .iter()
        .map(|(_, def)| def.span.clone())
        .collect();
    definitions.sort_by_key(|span| span.start);
    let references = definitions
        .into_iter()
        .map(|span| src[span].trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    let mut blocks = Vec::new();
    let mut depth = 0usize;
    let mut lists = 0usize;
    // The start of the top-level list being read and the index of its first block.
    let mut list: Option<(Option<u64>, usize)> = None;
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => {
                match (depth, &tag) {
                    (0, Tag::List(start)) => list = Some((*start, blocks.len())),
                    (0, Tag::MetadataBlock(_)) => {}
                    (0, _) => blocks.push(MdBlock { range, list: None }),
                    (1, Tag::Item) if list.is_some() => blocks.push(MdBlock {
                        range,
                        list: Some(ListPlace {
                            list: lists,
                            number: None,
                            digits: 0.0,
                        }),
                    }),
                    _ => {}
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some((start, first)) = list.take()
                {
                    let items = &mut blocks[first..];
                    let count = items.len() as u64;
                    let digits = ((start.unwrap_or(0) + count).max(1) as f32).log10().ceil();
                    for (k, block) in items.iter_mut().enumerate() {
                        block.list = Some(ListPlace {
                            list: lists,
                            number: start.map(|s| s + k as u64),
                            digits,
                        });
                    }
                    lists += 1;
                }
            }
            Event::Rule if depth == 0 => blocks.push(MdBlock { range, list: None }),
            _ => {}
        }
    }
    MdBlocks { blocks, references }
}

/// The quote that anchors a comment on `block`, a block's Markdown source: trimmed and cut to
/// [`caps::ANNOTATION_QUOTE`] bytes.
pub fn annotation_quote(block: &str) -> String {
    caps::clamp(block, caps::ANNOTATION_QUOTE)
        .trim_end()
        .to_owned()
}

/// The quotes of every block of `src`, in order.
pub fn block_quotes(src: &str) -> Vec<String> {
    md_blocks(src)
        .blocks
        .iter()
        .map(|b| annotation_quote(&src[b.range.clone()]))
        .collect()
}

/// A line of a diff: kept, added, removed, or a gap of unchanged lines left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Same(String),
    Added(String),
    Removed(String),
    Gap,
}

/// The changed lines of `new` against `old` with 2 lines of context each; a `Gap` stands for
/// two or more unchanged lines left out before, between and after the groups (a single one is
/// shown instead). Empty when the texts are equal. Lines carry no trailing newline.
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    use similar::{ChangeTag, TextDiff};

    // A last line without its newline is the same line as one with it.
    let ended = |text: &str| {
        let mut text = text.to_owned();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text
    };
    let (old, new) = (ended(old), ended(new));
    let diff = TextDiff::from_lines(&old, &new);
    let old_slices = diff.old_slices();
    let trim = |line: &str| line.trim_end_matches(['\n', '\r']).to_owned();
    let mut out = Vec::new();
    let skip = |out: &mut Vec<DiffLine>, from: usize, to: usize| match to.saturating_sub(from) {
        0 => {}
        1 => out.push(DiffLine::Same(trim(old_slices[from]))),
        _ => out.push(DiffLine::Gap),
    };
    let groups = diff.grouped_ops(2);
    let mut shown_to = 0;
    for group in &groups {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        skip(&mut out, shown_to, first.old_range().start);
        for op in group {
            for change in diff.iter_changes(op) {
                let line = trim(change.value());
                out.push(match change.tag() {
                    ChangeTag::Equal => DiffLine::Same(line),
                    ChangeTag::Insert => DiffLine::Added(line),
                    ChangeTag::Delete => DiffLine::Removed(line),
                });
            }
        }
        shown_to = last.old_range().end;
    }
    if !groups.is_empty() {
        skip(&mut out, shown_to, old_slices.len());
    }
    out
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
    /// The announced release is not one this build installs.
    UpToDate,
    /// Look up this release.
    Ask(Lookup),
}

/// Decides a check from where the updater stands, the channel it follows, whether the member
/// asked for GitHub's latest release (`latest`), the release the servers announced and this
/// build's version. The dev channel always asks for the newest dev build: announcements name
/// stable releases only.
pub fn next_check(
    updater: &UpdateState,
    channel: Channel,
    latest: bool,
    announced: Option<semver::Version>,
    current: &semver::Version,
) -> Check {
    if channel == Channel::Dev {
        return Check::Ask(Lookup::Dev);
    }
    let target = if latest { None } else { announced };
    if let UpdateState::Available { release, .. } = updater
        && !latest
        && target.as_ref().is_some_and(|t| *t <= release.version)
    {
        return Check::Skip;
    }
    if target
        .as_ref()
        .is_some_and(|version| !schwitzer_release::is_update(current, version))
    {
        return Check::UpToDate;
    }
    Check::Ask(target.map_or(Lookup::Latest, Lookup::Version))
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

/// Whether a server answers, with the reason when it does not.
#[derive(Debug, Clone, Default)]
pub struct ServerStatus {
    pub online: bool,
    pub error: Option<String>,
}

/// The server's line for the selected room's `server`, `None` while no room is selected;
/// `known` is what was learned about the server, `None` before it said anything.
pub fn server_status(server: Option<&ServerAddr>, known: Option<&ServerStatus>) -> Status {
    let (dot, word, detail) = match server {
        None => (Dot::Unknown, "no room", "No room selected".to_owned()),
        Some(addr) => {
            let server = addr.authority();
            match known {
                Some(ServerStatus { online: true, .. }) => {
                    (Dot::Up, "online", format!("{server} online"))
                }
                Some(ServerStatus { error: Some(e), .. }) => {
                    (Dot::Down, "offline", format!("{server} offline: {e}"))
                }
                Some(_) => (Dot::Down, "offline", format!("{server} offline")),
                None => (
                    Dot::Unknown,
                    "connecting",
                    format!("Connecting to {server}"),
                ),
            }
        }
    };
    Status {
        name: "Server",
        dot,
        word,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use schwitzer_proto::{Claim, Priority, Project};

    use super::*;

    const BLOCKS_SRC: &str = "# H\n\npara\n\n- a\n  - nested\n- b\n\n1. x\n1. y\n\n```\ncode\n```\n\n[r]: http://e.org\n";

    #[test]
    fn markdown_splits_into_top_level_blocks_and_list_items() {
        assert_eq!(
            block_quotes(BLOCKS_SRC),
            [
                "# H",
                "para",
                "- a\n  - nested",
                "- b",
                "1. x",
                "1. y",
                "```\ncode\n```"
            ]
        );
        let blocks = md_blocks(BLOCKS_SRC);
        let places: Vec<_> = blocks.blocks.iter().map(|b| b.list).collect();
        assert_eq!(places[..2], [None, None]);
        for place in &places[2..4] {
            let place = place.unwrap();
            assert_eq!((place.list, place.number), (0, None));
        }
        for (place, number) in places[4..6].iter().zip([1, 2]) {
            assert_eq!(
                *place,
                Some(ListPlace {
                    list: 1,
                    number: Some(number),
                    digits: 1.0
                })
            );
        }
        assert_eq!(places[6], None);
        assert_eq!(blocks.references, "[r]: http://e.org");
    }

    #[test]
    fn a_long_block_is_quoted_up_to_the_cap() {
        let block = "x".repeat(5 * 1024);
        assert!(annotation_quote(&block).len() <= caps::ANNOTATION_QUOTE);
    }

    #[test]
    fn split_idea_takes_the_first_line_as_title() {
        assert_eq!(split_idea("  \n "), None);
        assert_eq!(split_idea("Idea"), Some(("Idea", "")));
        assert_eq!(
            split_idea("\n Title \r\nline 1\nline 2\n"),
            Some(("Title", "line 1\nline 2"))
        );
    }

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

    #[test]
    fn return_anchor_is_the_proposal_after_else_before_else_the_header() {
        let (t1, t2, t3) = (
            task(1, Stage::Proposed, None),
            task(2, Stage::Proposed, None),
            task(3, Stage::Proposed, None),
        );
        let all = [&t1, &t2, &t3];
        assert_eq!(
            return_anchor(&all, 2),
            Some(ReturnAnchor::Next("task-3".to_owned()))
        );
        assert_eq!(
            return_anchor(&all, 3),
            Some(ReturnAnchor::Above("task-2".to_owned()))
        );
        assert_eq!(
            return_anchor(&[&t1], 1),
            Some(ReturnAnchor::Above("group-proposals".to_owned()))
        );
        assert_eq!(return_anchor(&[&t1], 9), None);
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
            details_version: 1,
            plan_version: 0,
            suggestions: Vec::new(),
            annotations: Vec::new(),
            votes: Vec::new(),
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
    fn omp_start_follows_the_stage_and_the_own_claim() {
        let me = member(1);
        let state = room(vec![task(5, Stage::Accepted, None)]);
        let start = |t: &Task, me, local: &[LocalSession]| omp_start(&state, t, me, local);
        assert_eq!(
            start(&task(4, Stage::Idea, None), me, &[]),
            Some(OmpStart::WriteUp(4))
        );
        assert_eq!(
            start(&task(5, Stage::Accepted, None), me, &[]),
            Some(OmpStart::Plan(5))
        );
        let mut ready = task(6, Stage::Ready, None);
        assert_eq!(start(&ready, me, &[]), Some(OmpStart::Build(6)));
        ready.waits_for = vec![5];
        assert_eq!(start(&ready, me, &[]), None);
        for stage in [
            Stage::Proposed,
            Stage::Planned,
            Stage::Done,
            Stage::Rejected,
        ] {
            assert_eq!(start(&task(7, stage, None), me, &[]), None);
        }

        let token = ClaimToken([3; 16]);
        let claimed = task(8, Stage::Ready, Some(claim(Work::Build, token)));
        let mut a = local("a", "/x", vec![token]);
        a.active_ms = 1;
        let mut b = local("b", "/x", vec![token]);
        b.active_ms = 2;
        let other = local("c", "/x", vec![ClaimToken([4; 16])]);
        let sessions = [a, b, other];
        assert_eq!(
            start(&claimed, me, &sessions),
            Some(OmpStart::Resume("b".to_owned()))
        );
        assert_eq!(start(&claimed, member(2), &sessions), None);
        assert_eq!(start(&claimed, me, &[]), None);
        assert_eq!(
            start(&claimed, me, &[local("x;rm", "/x", vec![token])]),
            None
        );
    }

    #[test]
    fn omp_command_quotes_for_the_shell_it_runs_in() {
        assert_eq!(
            omp_command(&OmpStart::Plan(12), Some(Path::new("/tmp/it's")), false).as_deref(),
            Some(r"omp --cwd '/tmp/it'\''s' '/schwitzer plan 12 now'")
        );
        assert_eq!(
            omp_command(&OmpStart::Build(3), Some(Path::new(r"C:\code\repo")), true).as_deref(),
            Some(r#"omp --cwd "C:\code\repo" "/schwitzer build 3 now""#)
        );
        assert_eq!(
            omp_command(
                &OmpStart::WriteUp(7),
                Some(Path::new(r"\\wsl.localhost\Ubuntu\home\felix\repo")),
                true
            )
            .as_deref(),
            Some("omp --cwd '/home/felix/repo' '/schwitzer propose 7'")
        );
        assert_eq!(
            omp_command(&OmpStart::Resume("abc-1".to_owned()), None, false).as_deref(),
            Some("omp --resume abc-1")
        );
        assert_eq!(omp_command(&OmpStart::Plan(1), None, false), None);
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
    fn ideas_sort_by_votes_then_priority_then_number() {
        let mut none = task(1, Stage::Idea, None);
        none.priority = Priority::High;
        let mut two = task(2, Stage::Idea, None);
        two.votes = vec![member(1), member(2)];
        let mut one = task(3, Stage::Idea, None);
        one.votes = vec![member(1)];
        let mut one_high = task(4, Stage::Idea, None);
        one_high.votes = vec![member(2)];
        one_high.priority = Priority::High;
        let state = room(vec![none, two, one, one_high]);
        let numbers: Vec<u64> = tasks_in(&state, 1, Group::Ideas)
            .iter()
            .map(|t| t.number)
            .collect();
        assert_eq!(numbers, [2, 4, 3, 1]);
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
            download: schwitzer_release::Download {
                url: String::new(),
                size: 0,
                sha256: [0; 32],
            },
        };
        let current = v("1.0.0");
        let idle = UpdateState::Idle;
        let offered = UpdateState::Available {
            release: release("1.2.0"),
            error: None,
        };
        let release_channel = Channel::Release;
        // No announcement: GitHub is asked.
        assert_eq!(
            next_check(&idle, release_channel, false, None, &current),
            Check::Ask(Lookup::Latest)
        );
        // An announcement above this build is looked up; one at or below it is not.
        assert_eq!(
            next_check(&idle, release_channel, false, Some(v("1.1.0")), &current),
            Check::Ask(Lookup::Version(v("1.1.0")))
        );
        assert_eq!(
            next_check(&idle, release_channel, false, Some(v("1.0.0")), &current),
            Check::UpToDate
        );
        assert_eq!(
            next_check(&idle, release_channel, false, Some(v("0.9.0")), &current),
            Check::UpToDate
        );
        // "Check now" ignores the announcement.
        assert_eq!(
            next_check(&idle, release_channel, true, Some(v("1.1.0")), &current),
            Check::Ask(Lookup::Latest)
        );
        // A release found above the announced one stays on offer, unless the member asks again.
        assert_eq!(
            next_check(&offered, release_channel, false, Some(v("1.1.0")), &current),
            Check::Skip
        );
        assert_eq!(
            next_check(&offered, release_channel, true, Some(v("1.1.0")), &current),
            Check::Ask(Lookup::Latest)
        );
        assert_eq!(
            next_check(&offered, release_channel, false, Some(v("1.3.0")), &current),
            Check::Ask(Lookup::Version(v("1.3.0")))
        );
        // A dev build on the release channel goes back to the announced stable release.
        assert_eq!(
            next_check(
                &idle,
                release_channel,
                false,
                Some(v("0.3.0")),
                &v("0.3.1-dev.5")
            ),
            Check::Ask(Lookup::Version(v("0.3.0")))
        );
        // The dev channel asks for the dev release whatever the servers announced.
        for announced in [None, Some(v("1.1.0")), Some(v("0.9.0"))] {
            assert_eq!(
                next_check(&idle, Channel::Dev, false, announced, &current),
                Check::Ask(Lookup::Dev)
            );
        }
    }

    #[test]
    fn a_diff_shows_changed_lines_in_their_context() {
        use DiffLine::{Added, Gap, Removed, Same};
        let s = |t: &str| t.to_owned();
        assert!(diff_lines("a\nb\n", "a\nb\n").is_empty());
        // A line added after a last line without newline leaves that line alone.
        assert_eq!(
            diff_lines("a\nb", "a\nb\nc"),
            [Same(s("a")), Same(s("b")), Added(s("c"))]
        );
        assert_eq!(
            diff_lines("a\nb\nc", "a\nB\nc"),
            [Same(s("a")), Removed(s("b")), Added(s("B")), Same(s("c"))]
        );
        let old: Vec<String> = (1..=20).map(|n| n.to_string()).collect();
        let mut new = old.clone();
        new[1] = s("two");
        new[17] = s("eighteen");
        let diff = diff_lines(&old.join("\n"), &new.join("\n"));
        assert_eq!(diff.iter().filter(|l| **l == Gap).count(), 1);
        let gap = diff.iter().position(|l| *l == Gap).unwrap();
        assert!(diff[..gap].contains(&Added(s("two"))));
        assert!(diff[gap..].contains(&Added(s("eighteen"))));
        assert_eq!(diff.first(), Some(&Same(s("1"))));
        assert_eq!(diff.last(), Some(&Same(s("20"))));
        // One left-out line is shown, not hidden behind a gap.
        assert_eq!(
            diff_lines("# T\n\none\ntwo", "# T\n\none\n2")[0],
            Same(s("# T"))
        );
        // A change in the middle of a long text leaves gaps on both sides.
        let mut middle = old.clone();
        middle[9] = s("ten");
        let diff = diff_lines(&old.join("\n"), &middle.join("\n"));
        assert_eq!(diff.first(), Some(&Gap));
        assert_eq!(diff.last(), Some(&Gap));
    }

    #[test]
    fn a_suggestion_is_outdated_once_its_part_moved_on() {
        let mut t = task(1, Stage::Proposed, None);
        let s = Suggestion {
            id: 1,
            part: Part::Details,
            version: 1,
            author: MemberId([1; 32]),
            agent: None,
            note: String::new(),
            at_ms: 0,
        };
        assert!(!outdated(&t, &s));
        t.plan_version = 3;
        assert!(!outdated(&t, &s));
        t.details_version = 2;
        assert!(outdated(&t, &s));
    }
}
