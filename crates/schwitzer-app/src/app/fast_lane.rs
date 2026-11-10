//! This member's fast lanes: the switches on a project and on a task, and the one place that
//! changes them.

use iced::Task;
use schwitzer_proto::{Decision, RoomId};

use super::{App, Message};
use crate::settings::Lanes;

impl App {
    /// Applies `change` to this member's fast lanes in the settings and in the copy the MCP
    /// tools read, and returns what it returns for the settings. Every change of the lanes goes
    /// through here, so the two never differ for longer than a flag the plugin set a moment ago.
    /// The caller saves the settings.
    pub(super) fn change_lanes<R>(&mut self, change: impl Fn(&mut Lanes) -> R) -> R {
        self.fast_lane.change(&change);
        change(&mut self.settings.lanes)
    }

    /// Puts the selected project into the fast lane or takes it out.
    pub(super) fn set_project_fast_lane(&mut self, on: bool) -> Task<Message> {
        if let Some((room, project)) = self.selected_ids() {
            self.change_lanes(|lanes| lanes.set_project(&room, project, on));
            self.save_settings();
        }
        Task::none()
    }

    /// Flips the fast lane of task `number` of the selected room; a proposal that enters it is
    /// accepted at once.
    pub(super) fn toggle_task_fast_lane(&mut self, number: u64) -> Task<Message> {
        let Some(room) = self.selected else {
            return Task::none();
        };
        let on = !self.settings.lanes.task(&room, number);
        self.change_lanes(|lanes| lanes.set_task(&room, number, on));
        self.save_settings();
        let proposed = self.selected_state().is_some_and(|state| {
            state
                .tasks
                .iter()
                .any(|t| t.number == number && t.stage == schwitzer_proto::Stage::Proposed)
        });
        if on && proposed {
            Task::done(Message::Decide(number, Decision::Accept))
        } else {
            Task::none()
        }
    }

    /// The plugin put task `task` of `room` into the fast lane or took it out.
    pub(super) fn plugin_fast_lane_task(
        &mut self,
        room: RoomId,
        task: u64,
        on: bool,
    ) -> Task<Message> {
        self.change_lanes(|lanes| lanes.set_task(&room, task, on));
        self.save_settings();
        Task::none()
    }
}
