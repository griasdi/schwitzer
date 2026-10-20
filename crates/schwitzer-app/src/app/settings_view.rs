//! The settings page: your name, the projects folders, the selected room and its projects, the
//! selected project and its linked repos, all rooms (switch, create, join), the omp connection,
//! in release builds the updates, and the about block with the licenses, each on its own slab;
//! and the first-start page with the projects folders.

use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::{RoomId, RoomKey, RoomState};

use super::icons::{Icon, Tone, icon};
use super::logic;
use super::pages::{frame, section};
use super::widgets::{
    dot, heading, icon_button, icon_text_button, id_button, input, muted, path_text, text_button,
};
use super::{App, FolderFor, Message, tilde};
use crate::about;
use crate::mcp::McpStatus;
use crate::omp_plugin::PluginState;
use crate::theme::{self, MEDIUM, SP_1, SP_2, SP_3, SP_4, TS_12, TS_14};
use crate::update::{self, UpdateState};
use crate::wsl;

pub fn view(app: &App) -> Element<'_, Message> {
    let mut body = column![section(you(app)), section(projects_folders(app))].spacing(SP_4);
    if app.selected_key().is_some() {
        body = body.push(section(this_room(app)));
    }
    if app.selected_project().is_some() {
        body = body.push(section(this_project(app)));
    }
    body = body.push(section(rooms(app))).push(section(agents(app)));
    if update::ENABLED {
        body = body.push(section(updates(app)));
    }
    body = body.push(section(about_block()));
    frame("settings-page", "Settings", body.into(), None)
}

/// The first-start page: the projects folders found, to keep, remove or add to, confirmed
/// with Continue or Back.
pub fn first_start(app: &App) -> Element<'_, Message> {
    let body = column![
        muted(
            "Choose the folders that hold your repos. Schwitzer filled in the usual ones it \
             found; you can change them later in Settings."
        ),
        section(projects_folders(app)),
    ]
    .spacing(SP_3);
    let footer = row![
        Space::new().width(Length::Fill),
        text_button(
            "projects-dirs-confirm",
            "Continue",
            Some(Message::ConfirmProjectsDirs),
            theme::button_primary
        ),
    ]
    .spacing(SP_2);
    frame(
        "first-start-page",
        "Your repos",
        body.into(),
        Some(footer.into()),
    )
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

fn projects_folders(app: &App) -> Element<'_, Message> {
    let folders = app.settings.projects_dirs.as_deref().unwrap_or_default();
    let mut rows = column![].spacing(2);
    for (index, path) in folders.iter().enumerate() {
        let remove = icon_button(
            format!("projects-dir-remove-{index}"),
            Icon::Trash,
            Tone::Muted,
            "Stop searching this folder",
            Some(Message::RemoveProjectsDir(index)),
            theme::button_ghost,
        );
        let folder = icon(Icon::Folder, 16.0, Tone::Muted);
        let line = match wsl::to_linux(path) {
            Some((distro, linux)) => row![folder, path_text(&linux), muted(distro), remove],
            None => row![folder, path_text(&tilde(path)), remove],
        };
        rows = rows.push(line.spacing(SP_2).align_y(Alignment::Center));
    }
    if folders.is_empty() {
        rows = rows.push(muted("No projects folder."));
    }
    let filled = !app.forms.new_projects_dir.trim().is_empty();
    column![
        heading("Projects folders"),
        rows,
        row![
            input(
                "projects-dir-input",
                "~/code",
                &app.forms.new_projects_dir,
                Message::ProjectsDirInput,
                Message::AddProjectsDir
            ),
            icon_button(
                "projects-dir-browse",
                Icon::FolderOpen,
                Tone::Muted,
                "Choose a folder",
                (!app.folder_dialog).then_some(Message::ChooseFolder(FolderFor::ProjectsDir)),
                theme::button_secondary,
            ),
            text_button(
                "projects-dir-add",
                "Add",
                filled.then_some(Message::AddProjectsDir),
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        muted(
            "Repos in these folders whose name matches a project, or contains it, are suggested \
             when you link a repo."
        ),
    ]
    .spacing(SP_2)
    .into()
}

/// The room's name, or `Room <first 8 hex digits>` before its server sent it.
fn room_label(key: &RoomKey, state: Option<&RoomState>) -> String {
    state.map_or_else(
        || format!("Room {}", &RoomId::of(&key.token).to_hex()[..8]),
        |state| state.name.clone(),
    )
}

fn this_room(app: &App) -> Element<'_, Message> {
    let Some(key) = app.selected_key() else {
        return Space::new().into();
    };
    let state = app.selected_state();
    let online = app.servers.get(&key.server).is_some_and(|s| s.online);
    let action = if online {
        icon_text_button(
            "leave-room",
            Icon::Leave,
            Tone::Danger,
            "Leave",
            Some(Message::LeaveRoom),
            theme::button_danger,
        )
    } else {
        icon_text_button(
            "remove-room",
            Icon::Trash,
            Tone::Danger,
            "Remove",
            Some(Message::RemoveRoom),
            theme::button_danger,
        )
    };
    let head = row![
        heading(room_label(key, state)).width(Length::Fill),
        icon_text_button(
            "settings-copy-key",
            Icon::Copy,
            Tone::Text,
            "Copy key",
            Some(Message::CopyKey),
            theme::button_secondary,
        ),
        action,
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    let mut col = column![head].spacing(SP_3);
    if !online {
        col = col.push(muted(format!(
            "{}:{} does not answer. Remove forgets the room on this machine only: the server keeps you as a member, and the key brings you back.",
            key.server.host, key.server.port
        )));
    }
    let Some(state) = state else {
        return col.into();
    };
    let selected = app.selected_project().map(|p| p.id);
    let mut projects = column![].spacing(2);
    for project in &state.projects {
        let linked = match app.settings.project_repos(&state.id, project.id).len() {
            0 => "No repo linked".to_owned(),
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
        projects = projects.push(id_button(
            format!("project-{}", project.id),
            container(line).padding([6.0, SP_2]),
            Some(Message::SelectProject(project.id)),
            theme::button_row,
            Length::Fill,
        ));
    }
    if state.projects.is_empty() {
        projects = projects.push(muted("No projects yet."));
    }
    let filled = !app.forms.new_project.trim().is_empty();
    col.push(text("Projects").size(TS_14).font(MEDIUM))
        .push(projects)
        .push(
            row![
                input(
                    "new-project-name",
                    "New project",
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
        )
        .into()
}

fn this_project(app: &App) -> Element<'_, Message> {
    let Some(project) = app.selected_project() else {
        return Space::new().into();
    };
    let linked = app.project_repos();
    let mut repos = column![].spacing(2);
    for (index, repo) in linked.iter().enumerate() {
        let state = app.plugins.get(repo).copied();
        let installed = state.is_some();
        let mut line = row![
            icon(Icon::Folder, 16.0, Tone::Muted),
            column![
                path_text(&repo.display().to_string()),
                muted(match state {
                    Some(PluginState::Current) => "omp plugin installed",
                    Some(PluginState::Outdated) => "omp plugin out of date",
                    None => "No omp plugin",
                }),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center);
        if state == Some(PluginState::Outdated) {
            line = line.push(icon_button(
                format!("repo-plugin-update-{index}"),
                Icon::Refresh,
                Tone::Ink,
                "Update the omp plugin in this repo",
                Some(Message::UpdatePlugin(index)),
                theme::button_ghost,
            ));
        }
        repos = repos.push(
            line.push(icon_button(
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
            ))
            .push(icon_button(
                format!("repo-remove-{index}"),
                Icon::Trash,
                Tone::Muted,
                "Unlink this repo",
                Some(Message::RemoveRepo(index)),
                theme::button_ghost,
            )),
        );
    }
    if linked.is_empty() {
        repos = repos.push(muted("No repo linked to this project yet."));
    }
    for (index, path) in logic::suggested_repos(&project.name, &app.project_dirs, linked)
        .into_iter()
        .enumerate()
    {
        repos = repos.push(
            row![
                icon(Icon::Folder, 16.0, Tone::Muted),
                path_text(&path.display().to_string()),
                text_button(
                    format!("repo-suggest-{index}"),
                    "Link",
                    Some(Message::LinkRepo(path.clone())),
                    theme::button_secondary,
                ),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
        );
    }
    let name = app.forms.project_name.trim();
    let renamed = !name.is_empty() && name != project.name;
    let filled = !app.forms.repo.trim().is_empty();
    let tasks = app.selected_state().map_or(0, |s| {
        s.tasks.iter().filter(|t| t.project == project.id).count()
    });
    let head = row![
        heading(&project.name).width(Length::Fill),
        icon_text_button(
            "delete-project",
            Icon::Trash,
            Tone::Danger,
            "Delete",
            (tasks == 0).then_some(Message::DeleteProject),
            theme::button_danger,
        ),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    let mut col = column![head].spacing(SP_3);
    if tasks > 0 {
        let count = if tasks == 1 {
            "1 task".to_owned()
        } else {
            format!("{tasks} tasks")
        };
        col = col.push(muted(format!(
            "Only an empty project can be deleted. Move its {count} into another project first."
        )));
    }
    col.push(
        row![
            input(
                "project-name-input",
                "Project name",
                &app.forms.project_name,
                Message::ProjectNameInput,
                Message::RenameProject
            ),
            text_button(
                "rename-project",
                "Rename",
                renamed.then_some(Message::RenameProject),
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    )
    .push(
        column![
            text("Linked repos").size(TS_14).font(MEDIUM),
            muted(
                "omp sessions running in these folders on this machine are visible to the \
                 room: title, project, runtime and tokens. Nothing else leaves your machine."
            ),
            muted(
                "The plug sets up omp in a repo: /schwitzer there plans, builds and proposes \
                 this project's tasks, and agents there get the room's Schwitzer tools."
            ),
        ]
        .spacing(SP_1),
    )
    .push(repos)
    .push(
        row![
            input(
                "repo-input",
                "/path/to/repo",
                &app.forms.repo,
                Message::RepoInput,
                Message::AddRepo
            ),
            icon_button(
                "repo-browse",
                Icon::FolderOpen,
                Tone::Muted,
                "Choose a folder",
                (!app.folder_dialog).then_some(Message::ChooseFolder(FolderFor::Repo)),
                theme::button_secondary,
            ),
            text_button(
                "repo-add",
                "Link",
                filled.then_some(Message::AddRepo),
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    )
    .into()
}

fn rooms(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(2);
    for (key, state) in &app.rooms {
        let id = RoomId::of(&key.token);
        let online = app.servers.get(&key.server).map(|s| s.online);
        let name = room_label(key, state.as_ref());
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
        Some(McpStatus::Listening { url }) => (Some(true), format!("Connection on {url}")),
        Some(McpStatus::PortInUse) => (
            Some(false),
            format!("Port {} is in use", app.settings.mcp_port),
        ),
        Some(McpStatus::Failed(e)) => (Some(false), e.clone()),
    };
    column![
        heading("omp"),
        row![
            icon(Icon::Plug, 16.0, Tone::Muted),
            dot(state),
            text(status).size(TS_14).width(Length::Fill),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        muted(
            "omp sessions in the shared repos reach this room through the omp plugin. The port \
             is mcp_port in settings.toml."
        ),
        text(format!(
            "omp sessions are read from {}",
            app.sessions_dirs()
                .iter()
                .map(|dir| logic::breakable_path(&dir.path.display().to_string()))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .size(TS_12)
        .style(theme::text_muted)
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        plugins(app),
    ]
    .spacing(SP_3)
    .into()
}

/// The omp plugins in all shared repos: how many are out of date, and auto-update.
fn plugins(app: &App) -> Element<'_, Message> {
    let installed = app.plugins.len();
    let outdated = app
        .plugins
        .values()
        .filter(|state| **state == PluginState::Outdated)
        .count();
    let summary = if installed == 0 {
        "No shared repo has the omp plugin.".to_owned()
    } else if outdated > 0 {
        format!("Out of date in {outdated} of {}.", repos(installed))
    } else {
        format!("Up to date in {}.", repos(installed))
    };
    let auto = on_off(
        "Update automatically",
        "plugins-auto-on",
        "plugins-auto-off",
        app.settings.auto_update_plugins,
        Message::AutoUpdatePlugins,
    );
    column![
        text("omp plugin").size(TS_14).font(MEDIUM),
        row![
            muted(summary).width(Length::Fill),
            text_button(
                "plugins-update-all",
                "Update all",
                (outdated > 0).then_some(Message::UpdateAllPlugins),
                theme::button_secondary,
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        auto,
        muted(
            "Rewrites an out-of-date plugin as soon as this app sees it. omp loads the new one \
             when it starts next."
        ),
    ]
    .spacing(SP_2)
    .into()
}

/// A label with On and Off segment buttons `on_id` and `off_id`.
fn on_off<'a>(
    label: &'a str,
    on_id: &'static str,
    off_id: &'static str,
    value: bool,
    message: fn(bool) -> Message,
) -> Element<'a, Message> {
    let segment = |id: &'static str, label: &'static str, v: bool| {
        id_button(
            id,
            container(text(label).size(TS_12).font(MEDIUM)).padding([SP_1, SP_3]),
            Some(message(v)),
            theme::button_segment(value == v),
            Length::Shrink,
        )
    };
    row![
        text(label).size(TS_14).width(Length::Fill),
        segment(on_id, "On", true),
        segment(off_id, "Off", false),
    ]
    .spacing(SP_1)
    .align_y(Alignment::Center)
    .into()
}

/// This build's version, the update check and what it found, and the two update switches.
fn updates(app: &App) -> Element<'_, Message> {
    let check = |enabled: bool| {
        text_button(
            "update-check",
            "Check now",
            enabled.then_some(Message::CheckLatestUpdate),
            theme::button_secondary,
        )
    };
    let (status, button) = match &app.updater {
        UpdateState::Idle => ("Not checked yet".to_owned(), check(true)),
        UpdateState::Checking => ("Checking for updates…".to_owned(), check(false)),
        UpdateState::UpToDate => ("Up to date".to_owned(), check(true)),
        UpdateState::Failed(e) => (format!("Cannot check for updates: {e}"), check(true)),
        UpdateState::Available { release, error } => (
            match error {
                None => format!("Version {} is available", release.version),
                Some(e) => format!("Version {} did not install: {e}", release.version),
            },
            text_button(
                "update-install",
                "Install",
                Some(Message::InstallUpdate),
                theme::button_secondary,
            ),
        ),
        UpdateState::Installing(release) => (
            format!("Installing version {}…", release.version),
            check(false),
        ),
        UpdateState::Installed(version) => (
            format!("Version {version} is installed. Restart to use it."),
            text_button(
                "update-restart",
                "Restart now",
                Some(Message::RestartForUpdate),
                theme::button_secondary,
            ),
        ),
    };
    column![
        heading("Updates"),
        row![text(status).size(TS_14).width(Length::Fill), button]
            .spacing(SP_2)
            .align_y(Alignment::Center),
        on_off(
            "Download and install",
            "auto-install-on",
            "auto-install-off",
            app.settings.auto_install_updates,
            Message::AutoInstallUpdates,
        ),
        on_off(
            "Restart",
            "auto-restart-on",
            "auto-restart-off",
            app.settings.auto_restart_updates,
            Message::AutoRestartUpdates,
        ),
        muted(
            "Installs the newest release your servers announce, or GitHub's latest one when no \
             server is reachable, and looks again every 5 minutes. Check now asks GitHub for its \
             latest release. After a restart you are back where you were, unsaved text included."
        ),
    ]
    .spacing(SP_3)
    .into()
}

fn about_block<'a>() -> Element<'a, Message> {
    column![
        row![
            heading("About").width(Length::Fill),
            muted(format!("Schwitzer {}", update::CURRENT)),
        ]
        .align_y(Alignment::Center),
        text("Free software under the MIT License or the Apache License 2.0, at your option.")
            .size(TS_14),
        muted(format!(
            "{}. Built on {} open source components, each under its own license.",
            about::COPYRIGHT,
            about::component_count()
        )),
        text_button(
            "licenses-open",
            "Licenses",
            Some(Message::OpenLicenses),
            theme::button_secondary,
        ),
    ]
    .spacing(SP_3)
    .into()
}

/// "1 repo", "2 repos".
fn repos(n: usize) -> String {
    if n == 1 {
        "1 repo".to_owned()
    } else {
        format!("{n} repos")
    }
}
