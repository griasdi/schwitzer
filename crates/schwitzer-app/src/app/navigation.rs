//! Where the window is: the page, the open task and the reading page. Opening and closing them,
//! Back, and what each change leaves of the drafts.

use iced::Task;
use iced::widget::operation;
use schwitzer_proto::{Part, RoomId};

use super::list::reveal;
use super::logic::Group;
use super::revisions::TaskPage;
use super::{App, Expanded, Message, Page, pages};

impl App {
    /// Opens the draft of a new task or idea, for Ctrl+N and the new-task buttons.
    pub(super) fn open_new_task(&mut self, idea: bool) -> Task<Message> {
        if !self.show_main_for_input() {
            return Task::none();
        }
        if matches!(self.expanded, Some(Expanded::Task(_))) {
            self.reset_drafts();
        }
        self.expanded = Some(Expanded::New { idea });
        self.collapsed.remove(if idea {
            &Group::Ideas
        } else {
            &Group::Proposals
        });
        Task::batch([operation::focus("proposal-title"), reveal("new-task")])
    }

    /// Focuses the quick idea input, for Ctrl+L.
    pub(super) fn focus_idea(&mut self) -> Task<Message> {
        if !self.show_main_for_input() {
            return Task::none();
        }
        operation::focus("quick-idea")
    }

    /// Opens task `number` of the selected room on the main page, in its project and group.
    pub(super) fn open_task(&mut self, number: u64) -> Task<Message> {
        let Some((room, project, group)) = self.selected_state().and_then(|s| {
            let task = s.task(number)?;
            Some((s.id, task.project, Group::of(task)))
        }) else {
            return Task::none();
        };
        if self.page == Page::FirstStart {
            return Task::none();
        }
        if self.choose_project(room, project) {
            self.sync_project_form();
        }
        self.page = Page::Main;
        if self.expanded != Some(Expanded::Task(number)) {
            self.reset_drafts();
        }
        self.expanded = Some(Expanded::Task(number));
        self.collapsed.remove(&group);
        reveal("open-task")
    }

    /// Opens task `number` in the list, or closes it when it is open.
    pub(super) fn toggle_task(&mut self, number: u64) -> Task<Message> {
        if self.expanded == Some(Expanded::Task(number)) {
            self.close_task();
            return Task::none();
        }
        self.reset_drafts();
        self.expanded = Some(Expanded::Task(number));
        reveal("open-task")
    }

    /// Back and Escape: closes the innermost thing that is open, else goes to the page's parent.
    pub(super) fn back(&mut self) -> Task<Message> {
        if self.page == Page::FirstStart {
            self.confirm_projects_dirs();
            return Task::none();
        }
        if self.confirm.take().is_some() {
            return Task::none();
        }
        if self.annotating.take().is_some() {
            return Task::none();
        }
        if self.task_page.take().is_some() {
            return reveal("open-task");
        }
        if self.expanded.take().is_some() {
            self.reset_drafts();
            return Task::none();
        }
        match self.page.parent() {
            Some(parent) => self.open_page(parent),
            None => Task::none(),
        }
    }

    /// Opens the reading page for `part` of the open task.
    pub(super) fn read(&mut self, part: Part) -> Task<Message> {
        let Some(Expanded::Task(task)) = self.expanded else {
            return Task::none();
        };
        self.task_page = Some(TaskPage::Reading { task, part });
        operation::snap_to(pages::BODY_ID, operation::RelativeOffset::START)
    }

    /// Brings up the main page for Ctrl+N and Ctrl+L, which type into its list; false when it
    /// shows no list (first start, no project, no repo, the plugin prompt).
    fn show_main_for_input(&mut self) -> bool {
        if self.page == Page::FirstStart || !self.shows_list() {
            return false;
        }
        if self.page != Page::Main {
            self.page = Page::Main;
            self.reset_drafts();
        }
        self.task_page = None;
        true
    }

    /// Opens `page` at its top. Drafts and a pending confirmation stay only when it is open
    /// already. The first-start page stays until its confirmation.
    pub(super) fn open_page(&mut self, page: Page) -> Task<Message> {
        if self.page == Page::FirstStart {
            return Task::none();
        }
        if self.page != page {
            self.reset_drafts();
            self.confirm = None;
        }
        self.expanded = None;
        self.page = page;
        self.sync_project_form();
        operation::snap_to(pages::BODY_ID, operation::RelativeOffset::START)
    }

    /// Shows project `id` of the selected room.
    pub(super) fn show_project(&mut self, id: u64) {
        if let Some(room) = self.selected {
            self.choose_project(room, id);
        }
        self.expanded = None;
        self.confirm = None;
        self.reset_drafts();
        self.sync_project_form();
        // A repo cloned a moment ago shows as soon as its project is opened.
        self.read_project_dirs();
    }

    pub(super) fn select(&mut self, room: Option<RoomId>) {
        if self.selected != room {
            self.selected = room;
            self.expanded = None;
            self.confirm = None;
            self.reset_drafts();
            self.sync_project_form();
        }
    }

    /// Closes the open task and drops its drafts.
    pub(super) fn close_task(&mut self) {
        self.expanded = None;
        self.reset_drafts();
    }

    /// Drops the open task's drafts and unfolds its Proposal and Plan: the next task opens with
    /// both shown.
    pub(super) fn reset_drafts(&mut self) {
        self.task_draft = None;
        self.plan_draft = None;
        self.proposal_folded = false;
        self.plan_folded = false;
        self.forms.note.clear();
        self.task_page = None;
        self.annotating = None;
        self.show_resolved = false;
    }
}
