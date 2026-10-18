//! The window chrome below the list: the button bar and the status bar with the room switcher.

use std::fmt;

use iced::widget::{Space, container, pick_list, row, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::RoomId;

use super::icons::{Bloom, Icon, Tone, icon};
use super::widgets::{self, dot, icon_button, muted, with_tip};
use super::{App, Message};
use crate::mcp::McpStatus;
use crate::theme::{self, SEMIBOLD, SP_1, SP_2, TS_12};

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

pub fn toolbar(app: &App) -> Element<'_, Message> {
    let in_room = app.selected_state().is_some();
    container(
        row![
            widgets::icon_text_button(
                "new-task",
                Icon::Plus,
                if in_room {
                    Tone::OnPrimary
                } else {
                    Tone::Muted
                },
                "New task",
                in_room.then_some(Message::OpenNewTask),
                theme::button_primary,
            ),
            icon_button(
                "copy-key",
                Icon::UserPlus,
                Tone::Text,
                "Copy room key to invite someone",
                in_room.then_some(Message::CopyKey),
                theme::button_ghost,
            ),
            Space::new().width(Length::Fill),
            icon_button(
                "settings",
                Icon::Settings,
                Tone::Text,
                "Settings  (Ctrl+,)",
                Some(Message::OpenSettings),
                theme::button_ghost,
            ),
        ]
        .spacing(SP_1)
        .align_y(Alignment::Center),
    )
    .padding([SP_1, SP_2])
    .width(Length::Fill)
    .style(theme::chrome)
    .into()
}

pub fn status_bar(app: &App) -> Element<'_, Message> {
    let server = app.selected_server_online();
    let bloom = match server {
        Some(true) => Bloom::Running,
        Some(false) => Bloom::Rejected,
        None => Bloom::Closed,
    };
    let choices: Vec<RoomChoice> = app.room_choices();
    let selected = choices.iter().find(|c| Some(c.id) == app.selected).cloned();
    let rooms: Element<'_, Message> = if choices.is_empty() {
        muted("No room").into()
    } else {
        container(
            pick_list(choices, selected, |c: RoomChoice| Message::SelectRoom(c.id))
                .text_size(TS_12)
                .padding([2.0, SP_2])
                .style(theme::picker)
                .menu_style(theme::picker_menu),
        )
        .id("room-picker")
        .into()
    };
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
        icon(Icon::Flower, 14.0, Tone::Bloom(bloom)),
        text(&app.settings.name).size(TS_12).font(SEMIBOLD),
        rooms,
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
        .style(theme::chrome)
        .into()
}
