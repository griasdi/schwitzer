//! The pages of this machine: the omp connection, the projects folders, and the updates.

use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length};

use super::{projects_folders, repos, switch_row};
use crate::app::icons::{Icon, Tone, icon};
use crate::app::logic;
use crate::app::pages::{frame, section};
use crate::app::widgets::{dot, heading, input, muted, path_text, text_button};
use crate::app::{App, Message};
use crate::mcp::McpStatus;
use crate::omp_plugin::PluginState;
use crate::settings;
use crate::theme::{self, SP_2, SP_4, TS_14};
use crate::update::{self, UpdateState};

/// The omp connection: its port, the plugins in the shared repos, and where sessions are read.
pub fn omp_page(app: &App) -> Element<'_, Message> {
    let (state, status) = match &app.mcp {
        None => (None, "Starting…".to_owned()),
        Some(McpStatus::Listening { url }) => (Some(true), format!("Connection on {url}")),
        Some(McpStatus::PortInUse) => (
            Some(false),
            format!("Port {} is in use", app.settings.mcp_port),
        ),
        Some(McpStatus::Failed(e)) => (Some(false), e.clone()),
    };
    let default_port = settings::DEFAULT_MCP_PORT.to_string();
    let changed =
        logic::parse_port(&app.forms.mcp_port).is_some_and(|p| p != app.settings.mcp_port);
    let connection = column![
        heading("Connection"),
        row![
            icon(Icon::Plug, 16.0, Tone::Muted),
            dot(state),
            text(status)
                .size(TS_14)
                .width(Length::Fill)
                .wrapping(text::Wrapping::WordOrGlyph),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        row![
            input(
                "mcp-port-input",
                &default_port,
                &app.forms.mcp_port,
                Message::McpPortInput,
                Message::SubmitMcpPort
            ),
            text_button(
                "save-mcp-port",
                "Save",
                changed.then_some(Message::SubmitMcpPort),
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        muted(
            "omp sessions in your linked repos reach Schwitzer through the plugin on this port. \
             A new port reaches every plugin with its next call; omp needs no restart."
        ),
    ]
    .spacing(SP_2);

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
    let plugins = column![
        heading("omp plugin"),
        row![
            text(summary).size(TS_14).width(Length::Fill),
            text_button(
                "plugins-update-all",
                "Update all",
                (outdated > 0).then_some(Message::UpdateAllPlugins),
                theme::button_secondary,
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        switch_row(
            "plugins-auto-toggle",
            None,
            "Update automatically",
            "Rewrites an out-of-date plugin as soon as Schwitzer sees it; omp loads it on its \
             next start.",
            app.settings.auto_update_plugins,
            Message::AutoUpdatePlugins,
            theme::button_row,
            [SP_2, SP_2],
        ),
    ]
    .spacing(SP_2);

    let mut files = column![
        heading("Session files"),
        muted("Schwitzer reads omp sessions from:"),
    ]
    .spacing(SP_2);
    for dir in app.sessions_dirs() {
        files = files.push(
            row![
                icon(Icon::Folder, 16.0, Tone::Muted),
                path_text(&dir.path.display().to_string()),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
        );
    }

    let body = column![section(connection), section(plugins), section(files)].spacing(SP_4);
    frame("omp-page", "omp", body.into(), None)
}

/// The folders searched for repos.
pub fn folders_page(app: &App) -> Element<'_, Message> {
    frame(
        "projects-folders-page",
        "Projects folders",
        section(projects_folders(app)),
        None,
    )
}

/// This build's version, the update check and what it found, and the two update switches.
pub fn updates_page(app: &App) -> Element<'_, Message> {
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
    let version = column![
        heading("This version"),
        row![
            text(status)
                .size(TS_14)
                .width(Length::Fill)
                .wrapping(text::Wrapping::WordOrGlyph),
            button
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
        muted(format!(
            "Schwitzer {}. Check now asks GitHub for its latest release.",
            update::CURRENT
        )),
    ]
    .spacing(SP_2);
    let automatic = column![
        heading("Automatic updates"),
        column![
            switch_row(
                "auto-install-toggle",
                None,
                "Install updates",
                "The newest release your servers announce, or GitHub's latest one; checked every \
                 5 minutes.",
                app.settings.auto_install_updates,
                Message::AutoInstallUpdates,
                theme::button_row,
                [SP_2, SP_2],
            ),
            switch_row(
                "auto-restart-toggle",
                None,
                "Restart after installing",
                "You come back where you were, unsaved text included.",
                app.settings.auto_restart_updates,
                Message::AutoRestartUpdates,
                theme::button_row,
                [SP_2, SP_2],
            ),
        ]
        .spacing(2),
    ]
    .spacing(SP_2);
    let body = column![section(version), section(automatic)].spacing(SP_4);
    frame("updates-page", "Updates", body.into(), None)
}
