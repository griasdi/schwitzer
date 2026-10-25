//! The task actions of the open room: propose, note an idea, pick, edit, plan, note, move,
//! priority, waits, and what each reply does to the forms and drafts. The slabs and the open
//! task are `list.rs` and `task_view.rs`.

use iced::Task;
use iced::widget::text_editor;
use schwitzer_client::ClientError;
use schwitzer_proto::{
    Decision, Priority, Request, Response, RoomId, Stage, Task as RoomTask, Work,
};

use super::{App, Expanded, Message, logic};
use crate::bridge::{call, fire};

/// The agent name a pick claims the build with; the picker's own agent finishes it.
const PICK_AGENT: &str = "omp";

impl App {
    pub(super) fn propose(&mut self) -> Task<Message> {
        let Some(Expanded::New { idea }) = self.expanded else {
            return Task::none();
        };
        let (Some(room), Some(project), title) = (
            self.selected,
            self.selected_project().map(|p| p.id),
            self.forms.proposal_title.trim(),
        ) else {
            return Task::none();
        };
        if title.is_empty() {
            return Task::none();
        }
        let (title, body) = (title.to_owned(), self.forms.proposal_body.text());
        let req = if idea {
            Request::AddIdea {
                room,
                project,
                title,
                body,
            }
        } else {
            Request::Propose {
                room,
                project,
                title,
                body,
                waits_for: Vec::new(),
            }
        };
        call(self.request(room, req), Message::Proposed)
    }

    pub(super) fn proposed(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(response) => {
                self.forms.proposal_title.clear();
                self.forms.proposal_body = text_editor::Content::new();
                if matches!(self.expanded, Some(Expanded::New { .. })) {
                    self.expanded = None;
                }
                let idea = matches!(&response, Response::Task(t) if t.stage == Stage::Idea);
                self.notify(if idea { "Idea added" } else { "Task proposed" });
            }
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    /// Sends the quick idea field's text as an idea: the first line is the title, the rest the
    /// details.
    pub(super) fn note_idea(&mut self) -> Task<Message> {
        if self.noting_idea.is_some() {
            return Task::none();
        }
        let text = self.forms.quick_idea.text();
        let Some((title, body)) = logic::split_idea(&text) else {
            return Task::none();
        };
        let (Some(room), Some(project)) = (self.selected, self.selected_project().map(|p| p.id))
        else {
            return Task::none();
        };
        let req = Request::AddIdea {
            room,
            project,
            title: title.to_owned(),
            body: body.to_owned(),
        };
        let task = call(self.request(room, req), Message::IdeaNoted);
        self.noting_idea = Some(text);
        task
    }

    pub(super) fn idea_noted(&mut self, result: Result<Response, String>) -> Task<Message> {
        let sent = self.noting_idea.take();
        match result {
            Ok(response) => {
                // Text typed while the request ran stays in the field.
                if Some(self.forms.quick_idea.text()) == sent {
                    self.forms.quick_idea = text_editor::Content::new();
                }
                match response {
                    Response::Task(t) => self.notify(&format!("Idea #{} added", t.number)),
                    _ => self.notify("Idea added"),
                }
            }
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    /// Claims the build of an approved task for this member and hands back the room name and the
    /// claimed task for the prompt.
    pub(super) fn pick(&mut self, number: u64) -> Task<Message> {
        if self.picking.is_some() {
            return Task::none();
        }
        let Some(state) = self.selected_state() else {
            return Task::none();
        };
        let Some(task) = state.tasks.iter().find(|t| t.number == number) else {
            return Task::none();
        };
        if !logic::pickable(task) || !task.open_waits(&state.tasks).is_empty() {
            return Task::none();
        }
        let (room, name) = (state.id, state.name.clone());
        self.picking = Some(number);
        let client = self.client.clone();
        call(
            async move {
                let claim = Request::Claim {
                    room,
                    work: Work::Build,
                    project: None,
                    task: Some(number),
                    agent: PICK_AGENT.to_owned(),
                };
                match client.request(room, claim).await? {
                    Response::Claimed(Some(task)) => Ok((name, task)),
                    other => Err(ClientError::Proto(format!("unexpected reply: {other:?}"))),
                }
            },
            Message::Picked,
        )
    }

    pub(super) fn picked(&mut self, result: Result<(String, RoomTask), String>) -> Task<Message> {
        self.picking = None;
        match result {
            Ok((room, task)) => match &task.claim {
                Some(claim) => {
                    let prompt = logic::pick_prompt(&room, &task, &claim.token);
                    self.notify(&format!("#{} is yours, prompt copied", task.number));
                    return iced::clipboard::write(prompt);
                }
                None => self.banner = Some("The claim came back empty".to_owned()),
            },
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    pub(super) fn save_task(&mut self) -> Task<Message> {
        let (Some((state, task)), Some(draft)) = (self.open(), &self.task_draft) else {
            return Task::none();
        };
        if draft.title.trim().is_empty() {
            return Task::none();
        }
        let room = state.id;
        let req = Request::EditTask {
            room,
            task: task.number,
            title: draft.title.clone(),
            body: draft.body.text(),
            plan: task.plan.clone(),
        };
        call(self.request(room, req), Message::TaskSaved)
    }

    /// Writes the open idea up into a proposal with the draft's title and body.
    pub(super) fn propose_idea(&self) -> Task<Message> {
        let (Some((state, task)), Some(draft)) = (self.open(), &self.task_draft) else {
            return Task::none();
        };
        if task.stage != Stage::Idea || draft.title.trim().is_empty() {
            return Task::none();
        }
        let room = state.id;
        let req = Request::ProposeIdea {
            room,
            task: task.number,
            title: draft.title.clone(),
            body: draft.body.text(),
            waits_for: Vec::new(),
        };
        call(self.request(room, req), Message::TaskSaved)
    }

    pub(super) fn task_saved(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(_) => self.task_draft = None,
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    pub(super) fn save_plan(&self) -> Task<Message> {
        let (Some((state, task)), Some(draft)) = (self.open(), &self.plan_draft) else {
            return Task::none();
        };
        let room = state.id;
        let req = Request::EditTask {
            room,
            task: task.number,
            title: task.title.clone(),
            body: task.body.clone(),
            plan: draft.text(),
        };
        call(self.request(room, req), Message::PlanSaved)
    }

    pub(super) fn plan_saved(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(_) => self.plan_draft = None,
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    pub(super) fn add_note(&mut self) -> Task<Message> {
        let Some((room, task)) = self.open_ids() else {
            return Task::none();
        };
        let text = self.forms.note.trim();
        if text.is_empty() {
            return Task::none();
        }
        let req = Request::AddNote {
            room,
            task,
            text: text.to_owned(),
            agent: None,
        };
        call(self.request(room, req), Message::NoteAdded)
    }

    pub(super) fn note_added(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(_) => self.forms.note.clear(),
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    pub(super) fn move_task(&self, project: u64) -> Task<Message> {
        let Some((state, task)) = self.open() else {
            return Task::none();
        };
        if task.project == project {
            return Task::none();
        }
        let room = state.id;
        let req = Request::MoveTask {
            room,
            task: task.number,
            project,
        };
        call(self.request(room, req), Message::TaskMoved)
    }

    pub(super) fn task_moved(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(Response::Task(task)) => {
                let target = self
                    .selected_state()
                    .and_then(|s| s.projects.iter().find(|p| p.id == task.project))
                    .map_or_else(|| format!("#{}", task.project), |p| p.name.clone());
                if self.expanded == Some(Expanded::Task(task.number)) {
                    self.expanded = None;
                    self.reset_drafts();
                }
                self.notify(&format!("#{} moved to {target}", task.number));
            }
            Ok(other) => self.banner = Some(format!("Unexpected reply: {other:?}")),
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    pub(super) fn set_priority(&self, priority: Priority) -> Task<Message> {
        match self.open_ids() {
            Some((room, task)) => fire(self.request(
                room,
                Request::SetPriority {
                    room,
                    task,
                    priority,
                },
            )),
            None => Task::none(),
        }
    }

    /// Sends the open task's waits after `edit`, when that changes them.
    pub(super) fn set_waits(&self, edit: impl FnOnce(&mut Vec<u64>)) -> Task<Message> {
        let Some((state, task)) = self.open() else {
            return Task::none();
        };
        let mut waits_for = task.waits_for.clone();
        edit(&mut waits_for);
        waits_for.sort_unstable();
        waits_for.dedup();
        if waits_for == task.waits_for {
            return Task::none();
        }
        let room = state.id;
        fire(self.request(
            room,
            Request::SetWaits {
                room,
                task: task.number,
                waits_for,
            },
        ))
    }

    pub(super) fn decide(&self, number: u64, decision: Decision) -> Task<Message> {
        match self.task_ids(number) {
            Some((room, task)) => fire(self.request(
                room,
                Request::Decide {
                    room,
                    task,
                    decision,
                },
            )),
            None => Task::none(),
        }
    }

    pub(super) fn unclaim(&self, number: u64) -> Task<Message> {
        match self.task_ids(number) {
            Some((room, task)) => fire(self.request(
                room,
                Request::Unclaim {
                    room,
                    task,
                    note: String::new(),
                },
            )),
            None => Task::none(),
        }
    }

    fn open_ids(&self) -> Option<(RoomId, u64)> {
        self.open().map(|(state, task)| (state.id, task.number))
    }

    /// The selected room's id with task `number`, for the buttons on a slab.
    fn task_ids(&self, number: u64) -> Option<(RoomId, u64)> {
        self.selected_state().map(|s| (s.id, number))
    }
}
