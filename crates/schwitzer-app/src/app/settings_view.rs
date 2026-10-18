//! The settings dialog: your name, the selected room and its shared repos, all rooms (switch,
//! create, join) and the MCP connection for agents.

use iced::widget::{Space, column, container, row, text, text_input};
use iced::{Alignment, Element, Length};
use schwitzer_proto::RoomId;

use super::dialogs::{self, frame};
use super::icons::{Icon, Tone, icon};
use super::widgets::{
    dot, heading, hrule, icon_button, icon_text_button, id_button, muted, text_button,
};
use super::{App, Message};
use crate::mcp::McpStatus;
use crate::theme::{self, MEDIUM, SP_1, SP_2, SP_3, SP_4, TS_12, TS_14};

pub fn view(app: &App) -> Element<'_, Message> {
    let mut body = column![you(app)].spacing(SP_4);
    if app.selected_state().is_some() {
        body = body.push(hrule()).push(this_room(app));
    }
    body = body
        .push(hrule())
        .push(rooms(app))
        .push(hrule())
        .push(agents(app));
    frame(
        "settings-dialog",
        dialogs::title("Settings"),
        body.into(),
        None,
        true,
    )
}

fn input<'a>(
    id: &'static str,
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Message,
    on_submit: Message,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .id(id)
        .on_input(on_input)
        .on_submit(on_submit)
        .padding(SP_2)
        .size(TS_14)
        .style(theme::input)
}

fn you(app: &App) -> Element<'_, Message> {
    let changed = app.forms.name.trim() != app.settings.name && !app.forms.name.trim().is_empty();
    column![
        heading("Your name"),
        row![
            input(
                "name-input",
                "Your name",
                &app.forms.name,
                Message::NameInput,
                Message::SubmitName
            ),
            text_button(
                "save-name",
                "Save",
                changed.then_some(Message::SubmitName),
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        muted("Shown on your tags in every room."),
    ]
    .spacing(SP_2)
    .into()
}

fn this_room(app: &App) -> Element<'_, Message> {
    let Some(state) = app.selected_state() else {
        return Space::new().into();
    };
    let mut repos = column![].spacing(2);
    for (index, repo) in app.room_repos().iter().enumerate() {
        let installed = app.plugin_repos.contains(repo);
        repos = repos.push(
            row![
                icon(Icon::Folder, 16.0, Tone::Muted),
                column![
                    text(repo.display().to_string()).size(TS_14),
                    muted(if installed {
                        "omp plugin installed"
                    } else {
                        "No omp plugin"
                    }),
                ]
                .spacing(2)
                .width(Length::Fill),
                icon_button(
                    format!("repo-plugin-{index}"),
                    Icon::Plug,
                    if installed { Tone::Text } else { Tone::Muted },
                    if installed {
                        "Remove the omp plugin from this repo"
                    } else {
                        "Install the omp plugin in this repo"
                    },
                    Some(if installed {
                        Message::RemovePlugin(index)
                    } else {
                        Message::InstallPlugin(index)
                    }),
                    theme::button_ghost,
                ),
                icon_button(
                    format!("repo-remove-{index}"),
                    Icon::Trash,
                    Tone::Muted,
                    "Stop sharing this repo",
                    Some(Message::RemoveRepo(index)),
                    theme::button_ghost,
                ),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
        );
    }
    if app.room_repos().is_empty() {
        repos = repos.push(muted("No repos shared with this room yet."));
    }
    let filled = !app.forms.repo.trim().is_empty();
    column![
        row![
            heading(&state.name).width(Length::Fill),
            icon_text_button(
                "settings-copy-key",
                Icon::Copy,
                Tone::Text,
                "Copy key",
                Some(Message::CopyKey),
                theme::button_secondary,
            ),
            icon_text_button(
                "leave-room",
                Icon::Leave,
                Tone::Danger,
                "Leave",
                Some(Message::LeaveRoom),
                theme::button_danger,
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        column![
            text("Shared repos").size(TS_14).font(MEDIUM),
            muted(
                "omp sessions running in these folders on this machine are visible to the \
                 room: title, project, runtime and tokens. Nothing else leaves your machine."
            ),
            muted(
                "The plug installs the omp plugin in a repo: /schwitzer in omp there lists the \
                 tasks waiting for a plan and plans one."
            ),
        ]
        .spacing(SP_1),
        repos,
        row![
            input(
                "repo-input",
                "/path/to/repo",
                &app.forms.repo,
                Message::RepoInput,
                Message::AddRepo
            ),
            text_button(
                "repo-add",
                "Add",
                filled.then_some(Message::AddRepo),
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    ]
    .spacing(SP_3)
    .into()
}

fn rooms(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(2);
    for (key, state) in &app.rooms {
        let id = RoomId::of(&key.token);
        let online = app.servers.get(&key.server).map(|s| s.online);
        let name = state.as_ref().map_or_else(
            || format!("Room {}", &id.to_hex()[..8]),
            |state| state.name.clone(),
        );
        let selected = app.selected == Some(id);
        let mut line = row![dot(online), text(name).size(TS_14).width(Length::Fill)]
            .spacing(SP_2)
            .align_y(Alignment::Center);
        if selected {
            line = line.push(icon(Icon::Check, 16.0, Tone::Ink));
        }
        line = line.push(muted(format!("{}:{}", key.server.host, key.server.port)));
        list = list.push(id_button(
            format!("room-{}", &id.to_hex()[..8]),
            container(line).padding([6.0, SP_2]),
            Some(Message::SelectRoom(id)),
            theme::button_row,
            Length::Fill,
        ));
    }
    if app.rooms.is_empty() {
        list = list.push(muted(
            "You are in no room yet. Create one or join with a key.",
        ));
    }
    let create_ok =
        !app.forms.server_url.trim().is_empty() && !app.forms.room_name.trim().is_empty();
    let join_ok = !app.forms.room_key.trim().is_empty();
    column![
        heading("Rooms"),
        list,
        text("Create a room").size(TS_14).font(MEDIUM),
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
        text("Join with a key").size(TS_14).font(MEDIUM),
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
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    ]
    .spacing(SP_3)
    .into()
}

fn agents(app: &App) -> Element<'_, Message> {
    let (state, status) = match &app.mcp {
        None => (None, "Starting…".to_owned()),
        Some(McpStatus::Listening { url }) => (Some(true), url.clone()),
        Some(McpStatus::PortInUse) => (
            Some(false),
            format!("Port {} is in use", app.settings.mcp_port),
        ),
        Some(McpStatus::Failed(e)) => (Some(false), e.clone()),
    };
    column![
        heading("Agents"),
        row![
            icon(Icon::Plug, 16.0, Tone::Muted),
            dot(state),
            text(status).size(TS_14).width(Length::Fill),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        muted(
            "Agents reach this room through the MCP server of this app. Paste the config into \
             your agent's MCP settings."
        ),
        row![icon_text_button(
            "copy-mcp-config",
            Icon::Copy,
            Tone::Text,
            "Copy MCP config",
            app.mcp_token.is_some().then_some(Message::CopyMcpConfig),
            theme::button_secondary,
        )],
        text(format!(
            "omp sessions are read from {}",
            app.settings.omp_sessions_dir.display()
        ))
        .size(TS_12)
        .style(theme::text_muted),
    ]
    .spacing(SP_3)
    .into()
}
