//! The header bar above the list and the status bar at the bottom.

use std::fmt;

use iced::widget::{Space, container, pick_list, row, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::RoomId;

use super::icons::{Icon, Tone};
use super::widgets::{bar_icon_button, dot, muted, with_tip};
use super::{App, Message};
use crate::mcp::McpStatus;
use crate::theme::{self, SEMIBOLD, SP_1, SP_2, TS_12, TS_14};

/// A room as the switcher lists it.
#[derive(Debug, Clone)]
pub struct RoomChoice {
    pub id: RoomId,
    pub name: String,
}

impl PartialEq for RoomChoice {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl fmt::Display for RoomChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// A project as the project picker lists it, and the entry that creates one.
#[derive(Debug, Clone)]
pub enum ProjectChoice {
    Project { id: u64, name: String },
    New,
}

impl PartialEq for ProjectChoice {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Project { id: a, .. }, Self::Project { id: b, .. }) => a == b,
            (Self::New, Self::New) => true,
            _ => false,
        }
    }
}

impl fmt::Display for ProjectChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project { name, .. } => f.write_str(name),
            Self::New => f.write_str("New project…"),
        }
    }
}

impl ProjectChoice {
    pub fn of(project: &schwitzer_proto::Project) -> Self {
        Self::Project {
            id: project.id,
            name: project.name.clone(),
        }
    }
}

/// The header bar: room and project switchers on the left, invite and settings on the right.
pub fn header(app: &App) -> Element<'_, Message> {
    let in_room = app.selected_state().is_some();
    let choices: Vec<RoomChoice> = app.room_choices();
    let selected = choices.iter().find(|c| Some(c.id) == app.selected).cloned();
    let rooms: Element<'_, Message> = if choices.is_empty() {
        muted("No room").into()
    } else {
        container(
            pick_list(choices, selected, |c: RoomChoice| Message::SelectRoom(c.id))
                .text_size(TS_14)
                .font(SEMIBOLD)
                .padding([SP_1, SP_2])
                .style(theme::picker)
                .menu_style(theme::picker_menu),
        )
        .id("room-picker")
        .into()
    };
    let mut bar = row![rooms];
    if let Some(state) = app.selected_state() {
        let choices: Vec<ProjectChoice> = state
            .projects
            .iter()
            .map(ProjectChoice::of)
            .chain([ProjectChoice::New])
            .collect();
        let selected = app.selected_project().map(ProjectChoice::of);
        bar = bar.push(
            container(
                pick_list(choices, selected, |choice| match choice {
                    ProjectChoice::Project { id, .. } => Message::SelectProject(id),
                    ProjectChoice::New => Message::OpenNewProject,
                })
                .placeholder("Project")
                .text_size(TS_14)
                .font(SEMIBOLD)
                .padding([SP_1, SP_2])
                .style(theme::picker)
                .menu_style(theme::picker_menu),
            )
            .id("project-picker"),
        );
    }
    container(
        bar.push(Space::new().width(Length::Fill))
            .push(bar_icon_button(
                "copy-key",
                Icon::UserPlus,
                Tone::Text,
                "Copy room key to invite someone",
                in_room.then_some(Message::CopyKey),
                theme::button_ghost,
            ))
            .push(bar_icon_button(
                "settings",
                Icon::Settings,
                Tone::Text,
                "Settings  (Ctrl+,)",
                Some(Message::OpenSettings),
                theme::button_ghost,
            ))
            .spacing(SP_1)
            .align_y(Alignment::Center),
    )
    .padding([SP_2, SP_2])
    .width(Length::Fill)
    .style(theme::bar)
    .into()
}

/// The status bar: my name, server and omp connection.
pub fn status_bar(app: &App) -> Element<'_, Message> {
    let server = app.selected_server_online();
    let server_label = match server {
        Some(true) => "online",
        Some(false) => "offline",
        None => "connecting",
    };
    let server_tip = app.selected_server_tip();
    let (omp_state, omp_tip) = match &app.mcp {
        None => (None, "omp connection starting".to_owned()),
        Some(McpStatus::Listening { url }) => (Some(true), format!("omp connection: {url}")),
        Some(McpStatus::PortInUse) => (
            Some(false),
            format!("omp connection: port {} is in use", app.settings.mcp_port),
        ),
        Some(McpStatus::Failed(e)) => (Some(false), format!("omp connection: {e}")),
    };
    let mut bar = row![
        text(&app.settings.name).size(TS_12).font(SEMIBOLD),
        Space::new().width(Length::Fill),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    if let Some(notice) = &app.notice {
        bar = bar.push(text(&notice.text).size(TS_12).style(theme::text_ink));
    } else {
        bar = bar.push(with_tip(
            row![dot(server), muted(server_label)]
                .spacing(SP_1)
                .align_y(Alignment::Center),
            server_tip,
        ));
    }
    bar = bar.push(with_tip(
        row![dot(omp_state), muted("omp")]
            .spacing(SP_1)
            .align_y(Alignment::Center),
        omp_tip,
    ));
    container(bar)
        .padding([SP_1, SP_2])
        .height(32)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .style(theme::bar)
        .into()
}
