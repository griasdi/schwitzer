//! The selected room's page (server and invite key, its projects, leave or remove) and the
//! page of all rooms (switch, join, create).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::RoomId;

use super::{confirm_row, room_label, view};
use crate::app::icons::{Icon, Tone, icon};
use crate::app::pages::{frame, section};
use crate::app::widgets::{
    dot, heading, icon_text_button, id_button, input, muted, path_text, text_button,
};
use crate::app::{App, Confirm, Message};
use crate::theme::{self, SP_2, SP_4, TS_14};

/// The selected room; the overview when no room is selected.
pub fn room_page(app: &App) -> Element<'_, Message> {
    let Some(key) = app.selected_key() else {
        return view(app);
    };
    let state = app.selected_state();
    let name = room_label(key, state);
    let address = format!("{}:{}", key.server.host, key.server.port);
    let online = app.servers.get(&key.server).map(|s| s.online);

    let server = column![
        heading("Server"),
        row![
            dot(online),
            path_text(&address),
            muted(match online {
                Some(true) => "online",
                Some(false) => "not reachable",
                None => "connecting",
            }),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        icon_text_button(
            "settings-copy-key",
            Icon::Copy,
            Tone::Text,
            "Copy invite key",
            Some(Message::CopyKey),
            theme::button_secondary,
        ),
        muted("Whoever has the key can join this room and sees all its tasks."),
    ]
    .spacing(SP_2);

    let mut body = column![section(server)].spacing(SP_4);

    if let Some(state) = state {
        let selected = app.selected_project().map(|p| p.id);
        let mut projects = column![].spacing(2);
        for project in &state.projects {
            let linked = match app.settings.project_repos(&state.id, project.id).len() {
                0 => "No repo linked on this machine".to_owned(),
                1 => "1 repo linked".to_owned(),
                n => format!("{n} repos linked"),
            };
            let mut line = row![
                icon(Icon::Folder, 16.0, Tone::Muted),
                column![text(&project.name).size(TS_14), muted(linked)]
                    .spacing(2)
                    .width(Length::Fill),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center);
            if selected == Some(project.id) {
                line = line.push(icon(Icon::Check, 16.0, Tone::Ink));
            }
            line = line.push(icon(Icon::ChevronRight, 16.0, Tone::Muted));
            projects = projects.push(id_button(
                format!("project-{}", project.id),
                container(line).padding([SP_2, SP_2]),
                Some(Message::OpenProject(project.id)),
                theme::button_row,
                Length::Fill,
            ));
        }
        if state.projects.is_empty() {
            projects = projects.push(muted("No projects yet."));
        }
        let filled = !app.forms.new_project.trim().is_empty();
        body = body.push(section(
            column![
                heading("Projects"),
                projects,
                row![
                    input(
                        "new-project-name",
                        "New project name",
                        &app.forms.new_project,
                        Message::NewProjectName,
                        Message::CreateProject
                    ),
                    text_button(
                        "create-project",
                        "Create",
                        filled.then_some(Message::CreateProject),
                        theme::button_secondary
                    ),
                ]
                .spacing(SP_2)
                .align_y(Alignment::Center),
            ]
            .spacing(SP_2),
        ));
    }

    let leave = if online == Some(false) {
        let action = if app.confirm == Some(Confirm::RemoveRoom) {
            confirm_row(
                format!("Remove {name}?"),
                "confirm-remove-room",
                Icon::Trash,
                "Remove",
                Message::RemoveRoom,
            )
        } else {
            icon_text_button(
                "remove-room",
                Icon::Trash,
                Tone::Danger,
                "Remove from this app",
                Some(Message::Ask(Confirm::RemoveRoom)),
                theme::button_danger,
            )
        };
        column![
            heading("Remove this room"),
            muted(format!(
                "{address} does not answer. Remove forgets the room on this machine only: the \
                 server keeps you as a member, and the key brings you back."
            )),
            action,
        ]
    } else {
        let action = if app.confirm == Some(Confirm::LeaveRoom) {
            confirm_row(
                format!("Leave {name}?"),
                "confirm-leave-room",
                Icon::Leave,
                "Leave",
                Message::LeaveRoom,
            )
        } else {
            icon_text_button(
                "leave-room",
                Icon::Leave,
                Tone::Danger,
                "Leave room",
                Some(Message::Ask(Confirm::LeaveRoom)),
                theme::button_danger,
            )
        };
        column![
            heading("Leave this room"),
            muted(
                "You leave the room's members and this app forgets its key. Copy the key first \
                 to come back later."
            ),
            action,
        ]
    };
    body = body.push(section(leave.spacing(SP_2)));

    frame("room-settings-page", name, body.into(), None)
}

/// All rooms to switch between, and the forms to join or create one.
pub fn rooms_page(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(2);
    for (key, state) in &app.rooms {
        let id = RoomId::of(&key.token);
        let online = app.servers.get(&key.server).map(|s| s.online);
        let mut line = row![
            dot(online),
            column![
                text(room_label(key, state.as_ref())).size(TS_14),
                muted(format!("{}:{}", key.server.host, key.server.port)),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center);
        if app.selected == Some(id) {
            line = line.push(icon(Icon::Check, 16.0, Tone::Ink));
        }
        line = line.push(icon(Icon::ChevronRight, 16.0, Tone::Muted));
        list = list.push(id_button(
            format!("room-{}", &id.to_hex()[..8]),
            container(line).padding([SP_2, SP_2]),
            Some(Message::OpenRoom(id)),
            theme::button_row,
            Length::Fill,
        ));
    }
    if app.rooms.is_empty() {
        list = list.push(muted("You are in no room yet."));
    }
    let create_ok =
        !app.forms.server_url.trim().is_empty() && !app.forms.room_name.trim().is_empty();
    let join_ok = !app.forms.room_key.trim().is_empty();
    let body = column![
        section(column![heading("Your rooms"), list].spacing(SP_2)),
        section(
            column![
                heading("Join a room"),
                muted("Paste the key someone shared with you."),
                row![
                    input(
                        "room-key",
                        "schwitzer://sha256:…/<key>",
                        &app.forms.room_key,
                        Message::RoomKeyInput,
                        Message::SubmitJoin
                    ),
                    text_button(
                        "join-submit",
                        "Join",
                        join_ok.then_some(Message::SubmitJoin),
                        theme::button_primary
                    ),
                ]
                .spacing(SP_2)
                .align_y(Alignment::Center),
            ]
            .spacing(SP_2)
        ),
        section(
            column![
                heading("Create a room"),
                muted("A new room on a schwitzer-server. Its address starts with weida://."),
                input(
                    "server-url",
                    "weida://sha256:…@host:7444",
                    &app.forms.server_url,
                    Message::ServerUrl,
                    Message::SubmitCreate
                ),
                row![
                    input(
                        "room-name",
                        "Room name",
                        &app.forms.room_name,
                        Message::RoomName,
                        Message::SubmitCreate
                    ),
                    text_button(
                        "create-submit",
                        "Create",
                        create_ok.then_some(Message::SubmitCreate),
                        theme::button_secondary
                    ),
                ]
                .spacing(SP_2)
                .align_y(Alignment::Center),
            ]
            .spacing(SP_2)
        ),
    ]
    .spacing(SP_4);
    frame("rooms-page", "Rooms", body.into(), None)
}
