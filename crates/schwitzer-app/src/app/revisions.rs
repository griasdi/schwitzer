//! The open task's page over the list: its details or plan for reading, its history, and the
//! review of an agent's suggestion. Loading the history and a suggestion, choosing a version,
//! restoring it into the editor, applying or discarding a suggestion, and keeping the page in
//! step with the room. The pages are drawn in `reading.rs` and `revisions_view.rs`.

use iced::Task;
use iced::widget::operation;
use schwitzer_proto::{Part, Request, Response, Revision, Suggestion, Task as RoomTask};

use super::logic::{self, DiffLine};
use super::{App, Message, Page, pages};
use crate::bridge::call;

/// What fills the window in place of the open task's list: its details or plan for reading, the
/// history of one part, or one of its suggestions. On the main page only, not kept over a
/// restart.
pub(super) enum TaskPage {
    Reading {
        task: u64,
        part: Part,
    },
    History {
        task: u64,
        part: Part,
        /// `None` while loading; newest first.
        versions: Option<Vec<Revision>>,
        /// The version shown unfolded with its diff.
        selected: u64,
        /// The selected version against the one before; computed on selection, never in `view`.
        diff: Vec<DiffLine>,
    },
    Suggestion {
        task: u64,
        id: u64,
        /// `None` while loading.
        loaded: Option<LoadedSuggestion>,
        /// An Apply or Discard is under way.
        busy: bool,
    },
}

pub(super) struct LoadedSuggestion {
    pub suggestion: Suggestion,
    pub title: String,
    pub text: String,
    /// The version of the part the diff was computed against.
    pub against: u64,
    pub diff: Vec<DiffLine>,
}

impl LoadedSuggestion {
    /// Recomputes the diff against the task's current text.
    fn rediff(&mut self, task: &RoomTask) {
        let part = self.suggestion.part;
        self.against = task.version(part);
        self.diff = logic::diff_lines(
            &current_text(task, part),
            &logic::part_text(part, &self.title, &self.text),
        );
    }
}

/// The task's current text of `part`, as diffs compare it.
fn current_text(task: &RoomTask, part: Part) -> String {
    match part {
        Part::Details => logic::part_text(part, &task.title, &task.body),
        Part::Plan => logic::part_text(part, "", &task.plan),
    }
}

/// The diff of version `selected` in `versions` (newest first) against the one listed after
/// it, or against nothing when no older one is listed.
fn diff_for(part: Part, versions: &[Revision], selected: u64) -> Vec<DiffLine> {
    let Some(i) = versions.iter().position(|v| v.version == selected) else {
        return Vec::new();
    };
    let new = &versions[i];
    let old = versions
        .get(i + 1)
        .map(|p| logic::part_text(part, &p.title, &p.text))
        .unwrap_or_default();
    logic::diff_lines(&old, &logic::part_text(part, &new.title, &new.text))
}

fn top() -> Task<Message> {
    operation::snap_to(pages::BODY_ID, operation::RelativeOffset::START)
}

impl App {
    fn history_request(&self, task: u64, part: Part) -> Task<Message> {
        let Some(room) = self.selected else {
            return Task::none();
        };
        let req = Request::History { room, task, part };
        call(self.request(room, req), move |r| {
            Message::HistoryLoaded(task, part, r)
        })
    }

    pub(super) fn open_history(&mut self, part: Part) -> Task<Message> {
        let Some((_, task)) = self.open() else {
            return Task::none();
        };
        let number = task.number;
        self.task_page = Some(TaskPage::History {
            task: number,
            part,
            versions: None,
            selected: task.version(part),
            diff: Vec::new(),
        });
        Task::batch([self.history_request(number, part), top()])
    }

    /// Closes the revisions page and shows `banner`: what was loading did not arrive.
    fn revisions_failed(&mut self, banner: String) {
        self.task_page = None;
        self.banner = Some(banner);
    }

    pub(super) fn history_loaded(
        &mut self,
        number: u64,
        loaded_part: Part,
        result: Result<Response, String>,
    ) -> Task<Message> {
        let Some(TaskPage::History {
            task,
            part,
            versions,
            selected,
            diff,
        }) = &mut self.task_page
        else {
            return Task::none();
        };
        if *task != number || *part != loaded_part {
            return Task::none();
        }
        match result {
            Ok(Response::History(list)) => {
                *selected = list.first().map_or(0, |v| v.version);
                *diff = diff_for(*part, &list, *selected);
                *versions = Some(list);
            }
            Ok(other) => self.revisions_failed(logic::unexpected(&other)),
            Err(e) => self.revisions_failed(e),
        }
        Task::none()
    }

    pub(super) fn select_version(&mut self, version: u64) -> Task<Message> {
        if let Some(TaskPage::History {
            part,
            versions: Some(list),
            selected,
            diff,
            ..
        }) = &mut self.task_page
        {
            *selected = version;
            *diff = diff_for(*part, list, version);
        }
        Task::none()
    }

    /// Puts an old version into the editor of its part; saving it makes it the newest version.
    pub(super) fn restore_version(&mut self, version: u64) -> Task<Message> {
        let Some(TaskPage::History {
            part,
            versions: Some(list),
            ..
        }) = &self.task_page
        else {
            return Task::none();
        };
        let Some(rev) = list.iter().find(|v| v.version == version) else {
            return Task::none();
        };
        let part = *part;
        let (title, text) = (rev.title.clone(), rev.text.clone());
        self.task_page = None;
        self.notify(&format!(
            "Version {version} is in the editor; Save makes it the newest version"
        ));
        match part {
            Part::Details => self.begin_task_edit(title, &text),
            Part::Plan => self.begin_plan_edit(&text),
        }
    }

    pub(super) fn review_suggestion(&mut self, id: u64) -> Task<Message> {
        let Some((state, task)) = self.open() else {
            return Task::none();
        };
        let (room, number) = (state.id, task.number);
        self.task_page = Some(TaskPage::Suggestion {
            task: number,
            id,
            loaded: None,
            busy: false,
        });
        let req = Request::GetSuggestion {
            room,
            task: number,
            suggestion: id,
        };
        Task::batch([
            call(self.request(room, req), move |r| {
                Message::SuggestionLoaded(id, r)
            }),
            top(),
        ])
    }

    pub(super) fn suggestion_loaded(
        &mut self,
        loaded_id: u64,
        result: Result<Response, String>,
    ) -> Task<Message> {
        let Some(TaskPage::Suggestion { id, .. }) = &self.task_page else {
            return Task::none();
        };
        if *id != loaded_id {
            return Task::none();
        }
        match result {
            Ok(Response::Suggestion {
                suggestion,
                title,
                text,
            }) => {
                let Some((_, task)) = self.open() else {
                    return Task::none();
                };
                let mut loaded = LoadedSuggestion {
                    suggestion,
                    title,
                    text,
                    against: 0,
                    diff: Vec::new(),
                };
                loaded.rediff(task);
                if let Some(TaskPage::Suggestion { loaded: slot, .. }) = &mut self.task_page {
                    *slot = Some(loaded);
                }
            }
            Ok(other) => self.revisions_failed(logic::unexpected(&other)),
            Err(e) => self.revisions_failed(e),
        }
        Task::none()
    }

    /// Sends Apply (`apply`) or Discard for the suggestion under review.
    pub(super) fn settle_suggestion(&mut self, id: u64, apply: bool) -> Task<Message> {
        let Some((room, number)) = self.open_ids() else {
            return Task::none();
        };
        let Some(TaskPage::Suggestion {
            id: shown, busy, ..
        }) = &mut self.task_page
        else {
            return Task::none();
        };
        if *shown != id || *busy {
            return Task::none();
        }
        *busy = true;
        if apply {
            let req = Request::ApplySuggestion {
                room,
                task: number,
                suggestion: id,
            };
            call(self.request(room, req), Message::SuggestionApplied)
        } else {
            let req = Request::DiscardSuggestion {
                room,
                task: number,
                suggestion: id,
            };
            call(self.request(room, req), Message::SuggestionDiscarded)
        }
    }

    pub(super) fn suggestion_settled(
        &mut self,
        result: Result<Response, String>,
        done: &str,
    ) -> Task<Message> {
        match result {
            Ok(_) => {
                if matches!(self.task_page, Some(TaskPage::Suggestion { .. })) {
                    self.task_page = None;
                }
                self.notify(done);
            }
            Err(e) => {
                if let Some(TaskPage::Suggestion { busy, .. }) = &mut self.task_page {
                    *busy = false;
                }
                self.banner = Some(e);
            }
        }
        Task::none()
    }

    /// Keeps the page in step with the room: closes it when the main page, the open task, the
    /// text read or the suggestion went away (Ctrl+N, Ctrl+, , a room or project switch, a task
    /// that vanished or lost its plan), reloads a history that a new version made stale and
    /// recomputes a suggestion's diff after its part changed.
    pub(super) fn settle_task_page(&mut self) -> Task<Message> {
        let Some(mut page) = self.task_page.take() else {
            return Task::none();
        };
        let page_task = match &page {
            TaskPage::Reading { task, .. }
            | TaskPage::History { task, .. }
            | TaskPage::Suggestion { task, .. } => *task,
        };
        let Some((_, open)) = self
            .open()
            .filter(|(_, t)| self.page == Page::Main && t.number == page_task)
        else {
            return Task::none();
        };
        let mut reload = None;
        match &mut page {
            TaskPage::Reading { part, .. } => {
                if !logic::has_text(open, *part) {
                    return Task::none();
                }
            }
            TaskPage::History {
                task,
                part,
                versions,
                ..
            } => {
                let newest = versions
                    .as_ref()
                    .map(|l| l.first().map_or(0, |v| v.version));
                if newest.is_some_and(|v| v != open.version(*part)) {
                    *versions = None;
                    reload = Some((*task, *part));
                }
            }
            TaskPage::Suggestion {
                id, loaded, busy, ..
            } => {
                if !*busy && !open.suggestions.iter().any(|s| s.id == *id) {
                    self.notify("The suggestion was applied or discarded");
                    return Task::none();
                }
                if let Some(l) = loaded
                    && l.against != open.version(l.suggestion.part)
                {
                    l.rediff(open);
                }
            }
        }
        self.task_page = Some(page);
        match reload {
            Some((task, part)) => self.history_request(task, part),
            None => Task::none(),
        }
    }
}
