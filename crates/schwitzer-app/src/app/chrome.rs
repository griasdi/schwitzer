//! The header bar above the list and the status bar at the bottom.

use std::fmt;

use iced::widget::{Row, Space, column, container, row, text};
use iced::{Alignment, Element, Length, Padding};
use schwitzer_proto::RoomId;

use super::icons::{Icon, Tone, icon};
use super::logic;
use super::widgets::{
    self, DOT, bar_icon_button, bar_tip, dot, id_button, member_tag, muted, with_card, with_tip,
};
use super::{App, Message};
use crate::omp_plugin::Coverage;
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, TS_12};

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

/// The header bar: room and project switchers on the left; members, invite and settings on the
/// right.
pub fn header(app: &App) -> Element<'_, Message> {
    let in_room = app.selected_state().is_some();
    let choices: Vec<RoomChoice> = app.room_choices();
    let selected = choices.iter().find(|c| Some(c.id) == app.selected).cloned();
    let rooms: Element<'_, Message> = if choices.is_empty() {
        muted("No room").into()
    } else {
        container(
            widgets::picker(choices, selected, |c: RoomChoice| Message::SelectRoom(c.id))
                .font(SEMIBOLD),
        )
        .id("room-picker")
        .into()
    };
    let mut bar = row![rooms];
    let mut members = None;
    if let Some(state) = app.selected_state() {
        let online = state.members.iter().filter(|m| m.online).count();
        let total = state.members.len();
        members = Some(bar_tip(
            id_button(
                "members",
                container(
                    row![
                        icon(Icon::Users, 16.0, Tone::Text),
                        text(format!("{online}/{total}")).size(TS_12).font(MEDIUM)
                    ]
                    .spacing(SP_1)
                    .align_y(Alignment::Center),
                )
                .padding(SP_2),
                Some(Message::ToggleMembers),
                theme::button_ghost_open(app.members_open),
                Length::Shrink,
            ),
            format!("Members: {online} of {total} online"),
        ));
        let choices: Vec<ProjectChoice> = state
            .projects
            .iter()
            .map(ProjectChoice::of)
            .chain([ProjectChoice::New])
            .collect();
        let selected = app.selected_project().map(ProjectChoice::of);
        bar = bar.push(
            container(
                widgets::picker(choices, selected, |choice| match choice {
                    ProjectChoice::Project { id, .. } => Message::SelectProject(id),
                    ProjectChoice::New => Message::OpenNewProject,
                })
                .placeholder("Project")
                .font(SEMIBOLD),
            )
            .id("project-picker"),
        );
    }
    container(
        bar.push(Space::new().width(Length::Fill))
            .push(members)
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

/// The member list under the header bar while it is unfolded: online members first, each as a
/// presence dot and name tag; the tooltip says who it is and whether they are online.
pub fn members(app: &App) -> Option<Element<'_, Message>> {
    if !app.members_open {
        return None;
    }
    let state = app.selected_state()?;
    let me = app.client.member();
    let entries = logic::members_by_presence(&state.members)
        .into_iter()
        .map(|m| {
            // The server sends an empty name for a member that never said hello.
            let name = if m.name.is_empty() {
                m.id.short()
            } else {
                m.name.clone()
            };
            let presence = if m.online { "online" } else { "offline" };
            let you = if m.id == me { " (you)" } else { "" };
            with_tip(
                row![
                    dot(m.online.then_some(true)),
                    member_tag(name.clone(), logic::member_hue(&m.id))
                ]
                .spacing(SP_1)
                .align_y(Alignment::Center),
                format!("{name}{you}, {presence}"),
            )
        });
    Some(
        container(
            Row::with_children(entries)
                .spacing(SP_2)
                .wrap()
                .vertical_spacing(SP_1),
        )
        .id("members-list")
        .padding(SP_2)
        .width(Length::Fill)
        .style(theme::bar)
        .into(),
    )
}

/// The status bar: my name, a notice, and one lamp for the server, the agent port and the omp
/// plugin, whose card spells each out.
pub fn status_bar(app: &App) -> Element<'_, Message> {
    let statuses = [
        app.server_status(),
        logic::mcp_status(app.mcp.as_ref(), app.settings.mcp_port),
        logic::omp_status(Coverage::of(app.project_repos(), &app.plugins)),
    ];
    let (state, word) = logic::overall(&statuses);
    let mut lamp = row![dot(state)].spacing(SP_1).align_y(Alignment::Center);
    let mut bar = row![
        text(&app.settings.name).size(TS_12).font(SEMIBOLD),
        Space::new().width(Length::Fill),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    if let Some(notice) = &app.notice {
        bar = bar.push(text(&notice.text).size(TS_12).style(theme::text_ink));
    } else {
        lamp = lamp.push(muted(word));
    }
    bar = bar.push(with_card(
        container(lamp).id("status-lamp"),
        status_card(statuses),
    ));
    container(bar)
        .padding([SP_1, SP_2])
        .height(theme::CONTROL)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .style(theme::bar)
        .into()
}

/// The lamp's card: per status its dot and name, and under the name its detail.
fn status_card<'a>(statuses: [logic::Status; 3]) -> Element<'a, Message> {
    column(statuses.into_iter().map(|s| {
        column![
            row![dot(s.dot), text(s.name).size(TS_12).font(SEMIBOLD)]
                .spacing(SP_1)
                .align_y(Alignment::Center),
            container(muted(s.detail).wrapping(text::Wrapping::WordOrGlyph)).padding(Padding {
                left: DOT + SP_1,
                ..Padding::ZERO
            }),
        ]
        .into()
    }))
    .spacing(SP_2)
    .into()
}
