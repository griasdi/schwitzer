//! Comments on single blocks of the open task's details or plan: writing one, saving it,
//! resolving and reopening. The cards and the editor are drawn in `reading.rs`.

use iced::Task;
use iced::widget::{operation, text_editor};
use schwitzer_proto::{Part, Request, Response};

use super::{App, Message, logic};
use crate::bridge::call;

/// The comment being written on a block of the open task; not kept over a restart.
pub(super) struct AnnotationDraft {
    pub task: u64,
    pub part: Part,
    pub quote: String,
    pub version: u64,
    pub text: text_editor::Content,
    pub sending: bool,
}

impl App {
    /// Opens the comment editor under block `index` of `part` of the open task.
    pub(super) fn annotate(&mut self, part: Part, index: usize) -> Task<Message> {
        let Some((_, task)) = self.open() else {
            return Task::none();
        };
        if !logic::editable(task) {
            return Task::none();
        }
        let Some(block) = self.open_md.part(part).blocks.get(index) else {
            return Task::none();
        };
        self.annotating = Some(AnnotationDraft {
            task: task.number,
            part,
            quote: block.quote.clone(),
            version: task.version(part),
            text: text_editor::Content::new(),
            sending: false,
        });
        operation::focus("comment-editor")
    }

    pub(super) fn annotation_action(&mut self, action: text_editor::Action) -> Task<Message> {
        if let Some(draft) = &mut self.annotating {
            draft.text.perform(action);
        }
        Task::none()
    }

    pub(super) fn save_annotation(&mut self) -> Task<Message> {
        let Some((room, _)) = self.open_ids() else {
            return Task::none();
        };
        let Some(draft) = &mut self.annotating else {
            return Task::none();
        };
        let text = draft.text.text();
        if draft.sending || text.trim().is_empty() {
            return Task::none();
        }
        draft.sending = true;
        let req = Request::Annotate {
            room,
            task: draft.task,
            part: draft.part,
            version: draft.version,
            quote: draft.quote.clone(),
            text,
        };
        call(self.request(room, req), Message::AnnotationSaved)
    }

    pub(super) fn annotation_saved(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(_) => {
                self.annotating = None;
                self.notify("Comment added");
            }
            Err(e) => {
                if let Some(draft) = &mut self.annotating {
                    draft.sending = false;
                }
                self.banner = Some(e);
            }
        }
        Task::none()
    }

    /// Marks comment `id` of the open task resolved, or open again.
    pub(super) fn resolve_annotation(&mut self, id: u64, resolved: bool) -> Task<Message> {
        let Some((room, number)) = self.open_ids() else {
            return Task::none();
        };
        let req = Request::ResolveAnnotation {
            room,
            task: number,
            annotation: id,
            resolved,
        };
        call(self.request(room, req), Message::AnnotationResolved)
    }
}
