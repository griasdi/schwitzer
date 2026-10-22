//! What the window showed when the app restarted for an update, so the new process opens
//! there: page, room, the open task or draft, unsaved text, the focused input and the
//! window's place.

use std::io;
use std::path::{Path, PathBuf};

use iced::advanced::widget::Id;
use iced::widget::text_editor::{self, Cursor, Position};
use iced::{Task, window};
use schwitzer_proto::{RoomId, now_ms};
use serde::{Deserialize, Serialize};

use super::logic::Group;
use super::{App, Expanded, Page, PendingFocus, Placement, TaskDraft};
use crate::update;

/// The file in the data directory; written just before the restart, removed when read.
const FILE: &str = "resume.json";
/// An older file is from an earlier run, not from the restart that just happened.
const MAX_AGE_MS: u64 = 120_000;
/// How long the new process waits for the restored element to show before giving up.
const FOCUS_WAIT_MS: u64 = 30_000;

/// The ids of the inputs whose focus survives a restart.
pub const FOCUSABLE: [&str; 15] = [
    "proposal-title",
    "proposal-body",
    "task-title",
    "task-body",
    "plan-editor",
    "note-input",
    "name-input",
    "mcp-port-input",
    "projects-dir-input",
    "new-project-name",
    "project-name-input",
    "repo-input",
    "server-url",
    "room-name",
    "room-key",
];
/// The text editors among [`FOCUSABLE`]; they keep their cursor, text inputs resume at the end.
pub const EDITORS: [&str; 3] = ["proposal-body", "task-body", "plan-editor"];

/// What the window showed when the app restarted for an update: the new process opens there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resume {
    /// The version that wrote the file; the new process says "Updated to …" when it differs.
    pub from_version: String,
    pub written_ms: u64,
    pub page: Page,
    pub selected: Option<RoomId>,
    pub expanded: Option<Expanded>,
    pub collapsed: Vec<Group>,
    pub sessions_open: bool,
    /// `false` in a file from a version without it.
    #[serde(default)]
    pub members_open: bool,
    /// `false` in a file from a version without it.
    #[serde(default)]
    pub proposal_folded: bool,
    /// `false` in a file from a version without it.
    #[serde(default)]
    pub plan_folded: bool,
    pub forms: FormsSnapshot,
    pub task_draft: Option<(String, Text)>,
    pub plan_draft: Option<Text>,
    /// Widget id of the focused input, one of [`FOCUSABLE`].
    pub focus: Option<String>,
    /// The window's place; `None` in a file from a version without it.
    #[serde(default)]
    pub placement: Option<Placement>,
    /// On an unconfirmed first-start page: its projects folders and entered WSL distros.
    /// `None` otherwise and in a file from a version without it.
    #[serde(default)]
    pub first_start: Option<(Vec<PathBuf>, Vec<String>)>,
}

/// Every `Forms` string field by its name, `proposal_body` as [`Text`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormsSnapshot {
    pub server_url: String,
    pub room_name: String,
    pub room_key: String,
    pub name: String,
    /// `None` in a file from a version without it.
    #[serde(default)]
    pub mcp_port: Option<String>,
    pub repo: String,
    pub new_project: String,
    pub project_name: String,
    pub new_projects_dir: String,
    pub proposal_title: String,
    pub proposal_body: Text,
    pub note: String,
}

/// A text editor's content and cursor (`text_editor::Cursor`, byte columns).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
    pub line: usize,
    pub column: usize,
    /// The other end of the selection: line and column.
    pub selection: Option<(usize, usize)>,
}

impl Text {
    pub fn of(content: &text_editor::Content) -> Self {
        let cursor = content.cursor();
        Self {
            text: content.text(),
            line: cursor.position.line,
            column: cursor.position.column,
            selection: cursor.selection.map(|p| (p.line, p.column)),
        }
    }

    /// The editor content with the cursor and selection put back, clamped into the text.
    pub fn content(&self) -> text_editor::Content {
        let mut content = text_editor::Content::with_text(&self.text);
        let position = clamp(&content, self.line, self.column);
        let selection = self.selection.map(|(l, c)| clamp(&content, l, c));
        content.move_to(Cursor {
            position,
            selection,
        });
        content
    }
}

/// `line` and byte `column` inside `content`: past the end goes to the last line, and a column
/// past the line's end or inside a character goes to the line's end.
fn clamp(content: &text_editor::Content, line: usize, column: usize) -> Position {
    let line = line.min(content.line_count().saturating_sub(1));
    let text = content.line(line).map(|l| l.text).unwrap_or_default();
    let column = if text.is_char_boundary(column) {
        column
    } else {
        text.len()
    };
    Position { line, column }
}

/// Writes `resume` into `dir` through a temporary file and a rename.
pub fn write(dir: &Path, resume: &Resume) -> io::Result<()> {
    let json = serde_json::to_vec(resume).map_err(io::Error::other)?;
    crate::settings::write_atomic(&dir.join(FILE), &json)
}

/// Removes the resume file from `dir`.
pub fn remove(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(FILE));
}

/// Reads and removes the resume file in `dir`; `None` without one, for an unreadable one and
/// for one written more than two minutes before `now_ms`.
pub fn take(dir: &Path, now_ms: u64) -> Option<Resume> {
    let path = dir.join(FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!("cannot read {}: {e}", path.display());
            remove(dir);
            return None;
        }
    };
    remove(dir);
    match serde_json::from_slice::<Resume>(&bytes) {
        Ok(resume) if now_ms.abs_diff(resume.written_ms) <= MAX_AGE_MS => Some(resume),
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("unreadable {}: {e}", path.display());
            None
        }
    }
}

/// Where the window is now: `None` without a window and for a minimized one, whose
/// position is no place on screen (Windows reports -32000).
pub(super) fn placement() -> Task<Option<Placement>> {
    window::latest().then(|id| {
        let Some(id) = id else {
            return Task::done(None);
        };
        window::is_minimized(id).then(move |minimized| {
            if minimized == Some(true) {
                return Task::done(None);
            }
            window::position(id).then(move |position| {
                window::size(id).then(move |size| {
                    window::is_maximized(id).map(move |maximized| {
                        Some(Placement {
                            position: position.map(|p| (p.x, p.y)),
                            size: (size.width, size.height),
                            maximized,
                        })
                    })
                })
            })
        })
    })
}

/// The resume file of a restart for an update, taken in `main` before the window opens: it
/// places the window, then `App::boot` opens the page.
pub struct Resumed(pub(super) Option<Resume>);

impl Resumed {
    /// Takes the resume file from the data directory (see [`take`]).
    pub fn take() -> Self {
        Self(take(&crate::settings::data_dir(), now_ms()))
    }

    /// `settings` with the window's size, position and maximized state from before the
    /// restart; unchanged without a file or without a placement in it.
    pub fn place(&self, settings: window::Settings) -> window::Settings {
        let Some(p) = self.0.as_ref().and_then(|r| r.placement) else {
            return settings;
        };
        window::Settings {
            size: iced::Size::new(p.size.0, p.size.1),
            position: p.position.map_or(window::Position::Default, |(x, y)| {
                window::Position::Specific(iced::Point::new(x, y))
            }),
            maximized: p.maximized,
            ..settings
        }
    }
}

impl App {
    /// What the window shows now; `focus` is the focused widget.
    pub(super) fn snapshot(&self, focus: Option<&Id>, placement: Option<Placement>) -> Resume {
        let forms = &self.forms;
        Resume {
            from_version: update::CURRENT.to_owned(),
            written_ms: now_ms(),
            page: self.page,
            selected: self.selected,
            expanded: self.expanded,
            collapsed: self.collapsed.iter().copied().collect(),
            sessions_open: self.sessions_open,
            members_open: self.members_open,
            proposal_folded: self.proposal_folded,
            plan_folded: self.plan_folded,
            forms: FormsSnapshot {
                server_url: forms.server_url.clone(),
                room_name: forms.room_name.clone(),
                room_key: forms.room_key.clone(),
                name: forms.name.clone(),
                mcp_port: Some(forms.mcp_port.clone()),
                repo: forms.repo.clone(),
                new_project: forms.new_project.clone(),
                project_name: forms.project_name.clone(),
                new_projects_dir: forms.new_projects_dir.clone(),
                proposal_title: forms.proposal_title.clone(),
                proposal_body: Text::of(&forms.proposal_body),
                note: forms.note.clone(),
            },
            task_draft: self
                .task_draft
                .as_ref()
                .map(|draft| (draft.title.clone(), Text::of(&draft.body))),
            plan_draft: self.plan_draft.as_ref().map(Text::of),
            focus: focus
                .and_then(|id| FOCUSABLE.into_iter().find(|f| *id == Id::new(f)))
                .map(str::to_owned),
            placement,
            first_start: self.first_start.then(|| {
                (
                    self.settings.projects_dirs.clone().unwrap_or_default(),
                    self.settings.wsl_distros.clone(),
                )
            }),
        }
    }

    /// Opens what `r` describes. The room and the open task or draft come back only when the
    /// room is still joined.
    pub(super) fn apply_resume(&mut self, r: Resume) {
        self.page = r.page;
        self.sessions_open = r.sessions_open;
        self.members_open = r.members_open;
        self.collapsed = r.collapsed.into_iter().collect();
        let joined = r.selected.filter(|room| self.rooms.contains(*room));
        if let Some(room) = joined {
            // Not `select`, which drops the drafts.
            self.selected = Some(room);
            self.expanded = r.expanded;
            self.proposal_folded = r.proposal_folded;
            self.plan_folded = r.plan_folded;
            self.task_draft = r.task_draft.map(|(title, body)| TaskDraft {
                title,
                body: body.content(),
            });
            self.plan_draft = r.plan_draft.as_ref().map(Text::content);
            self.sync_project_form();
        }
        if self.first_start
            && let Some((dirs, distros)) = r.first_start
        {
            self.settings.projects_dirs = Some(dirs);
            self.settings.wsl_distros = distros;
            self.read_project_dirs();
        }
        let forms = r.forms;
        self.forms.server_url = forms.server_url;
        self.forms.room_name = forms.room_name;
        self.forms.room_key = forms.room_key;
        self.forms.name = forms.name;
        if let Some(port) = forms.mcp_port {
            self.forms.mcp_port = port;
        }
        self.forms.repo = forms.repo;
        self.forms.new_project = forms.new_project;
        self.forms.project_name = forms.project_name;
        self.forms.new_projects_dir = forms.new_projects_dir;
        self.forms.proposal_title = forms.proposal_title;
        self.forms.proposal_body = forms.proposal_body.content();
        self.forms.note = forms.note;
        if r.from_version != update::CURRENT {
            self.notify(&format!("Updated to {}", update::CURRENT));
        }
        let focus = r
            .focus
            .and_then(|f| FOCUSABLE.into_iter().find(|id| *id == f));
        if focus.is_some() || self.expanded.is_some() {
            self.resume_focus = Some(PendingFocus {
                focus,
                until_ms: now_ms() + FOCUS_WAIT_MS,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(text: &str, line: usize, column: usize, selection: Option<(usize, usize)>) -> Text {
        Text {
            text: text.to_owned(),
            line,
            column,
            selection,
        }
    }

    fn resume(written_ms: u64) -> Resume {
        let room = RoomId([7; 32]);
        Resume {
            from_version: "0.1.0".to_owned(),
            written_ms,
            page: Page::Main,
            selected: Some(room),
            expanded: Some(Expanded::New { idea: true }),
            collapsed: vec![Group::Closed],
            sessions_open: false,
            members_open: true,
            proposal_folded: true,
            plan_folded: true,
            forms: FormsSnapshot {
                server_url: "weida://x".to_owned(),
                room_name: String::new(),
                room_key: String::new(),
                name: "alice".to_owned(),
                mcp_port: Some("13401".to_owned()),
                repo: String::new(),
                new_project: String::new(),
                project_name: "smoke".to_owned(),
                new_projects_dir: String::new(),
                proposal_title: "Restart test".to_owned(),
                proposal_body: text("hello\nworld", 1, 2, None),
                note: "half a note".to_owned(),
            },
            task_draft: Some(("Title".to_owned(), text("body", 0, 4, Some((0, 1))))),
            plan_draft: None,
            focus: Some("proposal-body".to_owned()),
            placement: Some(Placement {
                position: Some((-8.0, 120.5)),
                size: (520.0, 700.0),
                maximized: true,
            }),
            first_start: Some((vec![PathBuf::from("/code")], vec!["Ubuntu".to_owned()])),
        }
    }

    #[test]
    fn written_resume_is_taken_once() {
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        write(dir.path(), &resume(now)).unwrap();
        assert!(!dir.path().join("resume.json.tmp").exists());
        assert_eq!(take(dir.path(), now + 1_000), Some(resume(now)));
        assert!(!dir.path().join(FILE).exists());
        assert_eq!(take(dir.path(), now + 1_000), None);
    }

    #[test]
    fn stale_resume_is_dropped_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        write(dir.path(), &resume(now - MAX_AGE_MS - 1)).unwrap();
        assert_eq!(take(dir.path(), now), None);
        assert!(!dir.path().join(FILE).exists());
    }

    #[test]
    fn resume_from_an_older_version_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        let mut old = serde_json::to_value(resume(now)).unwrap();
        let fields = old.as_object_mut().unwrap();
        fields.remove("placement");
        fields.remove("members_open");
        fields.remove("proposal_folded");
        fields.remove("plan_folded");
        fields.remove("first_start");
        fields.insert("chosen_projects".to_owned(), serde_json::json!([]));
        fields["forms"].as_object_mut().unwrap().remove("mcp_port");
        std::fs::write(dir.path().join(FILE), old.to_string()).unwrap();
        assert_eq!(
            take(dir.path(), now + 1_000),
            Some(Resume {
                placement: None,
                first_start: None,
                members_open: false,
                proposal_folded: false,
                plan_folded: false,
                forms: FormsSnapshot {
                    mcp_port: None,
                    ..resume(now).forms
                },
                ..resume(now)
            })
        );
    }

    #[test]
    fn garbage_is_dropped_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "{not json").unwrap();
        assert_eq!(take(dir.path(), now_ms()), None);
        assert!(!dir.path().join(FILE).exists());
    }

    #[test]
    fn out_of_range_cursor_is_clamped() {
        // Line 9 is past the end; column 3 is inside "é"; column 99 is past line 0's end.
        let back = Text::of(&text("ab\ncdé", 9, 3, Some((0, 99))).content());
        assert_eq!(back.text, "ab\ncdé");
        assert_eq!((back.line, back.column), (1, "cdé".len()));
        assert_eq!(back.selection, Some((0, 2)));
    }

    #[test]
    fn text_keeps_its_cursor_and_selection() {
        for t in [
            text("hello world", 0, 6, None),
            text("first\nsecond line\nthird", 1, 3, Some((2, 5))),
            text("", 0, 0, None),
        ] {
            assert_eq!(Text::of(&t.content()), t);
        }
    }
}
