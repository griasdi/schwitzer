//! The selected project's page: its repos on this machine and their omp plugin, your agents'
//! commits, its name, and deleting it.

use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length};

use super::{Switch, switch_row, view};
use crate::app::icons::{Icon, Tone, icon};
use crate::app::logic;
use crate::app::pages::{frame, section};
use crate::app::widgets::{
    confirm_row, heading, icon_button, icon_text_button, input, muted, path_text, text_button,
};
use crate::app::{App, Confirm, FolderFor, Message};
use crate::omp_plugin::PluginState;
use crate::theme::{self, SEMIBOLD, SP_2, SP_4, TS_12};

/// The selected project; the overview when there is none.
pub fn project_page(app: &App) -> Element<'_, Message> {
    let Some(project) = app.selected_project() else {
        return view(app);
    };

    let linked = app.project_repos();
    let mut repos = column![].spacing(2);
    for (index, repo) in linked.iter().enumerate() {
        let state = app.plugins.get(repo).copied();
        let caption: Element<'_, Message> = match state {
            Some(PluginState::Current) => muted("omp plugin installed").into(),
            Some(PluginState::Outdated) => text("omp plugin out of date")
                .size(TS_12)
                .style(theme::text_ink)
                .into(),
            None => muted("No omp plugin").into(),
        };
        let plugin = match state {
            None => text_button(
                format!("repo-plugin-{index}"),
                "Install",
                Some(Message::InstallPlugin(index)),
                theme::button_secondary,
            ),
            Some(PluginState::Outdated) => text_button(
                format!("repo-plugin-update-{index}"),
                "Update",
                Some(Message::UpdatePlugin(index)),
                theme::button_secondary,
            ),
            Some(PluginState::Current) => icon_button(
                format!("repo-plugin-{index}"),
                Icon::Plug,
                Tone::Muted,
                "Remove the omp plugin from this repo",
                Some(Message::RemovePlugin(index)),
                theme::button_ghost,
            ),
        };
        repos = repos.push(
            row![
                icon(Icon::Folder, 16.0, Tone::Muted),
                column![path_text(&repo.display().to_string()), caption]
                    .spacing(2)
                    .width(Length::Fill),
                plugin,
                icon_button(
                    format!("repo-remove-{index}"),
                    Icon::Trash,
                    Tone::Muted,
                    "Unlink this repo",
                    Some(Message::RemoveRepo(index)),
                    theme::button_ghost,
                ),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
        );
    }
    if linked.is_empty() {
        repos = repos.push(muted("No repo linked yet. Link one below."));
    }
    let suggested = logic::suggested_repos(&project.name, &app.project_dirs, linked);
    if !suggested.is_empty() {
        repos = repos.push(text("Suggested").size(TS_12).font(SEMIBOLD));
    }
    for (index, path) in suggested.into_iter().enumerate() {
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
    let filled = !app.forms.repo.trim().is_empty();
    let repos_section = column![
        heading("Repos on this machine"),
        muted(
            "omp sessions in these folders are shared with the room: title, runtime and tokens. \
             Nothing else leaves this machine."
        ),
        repos,
        row![
            input(
                "repo-input",
                "Path to a repo",
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
        muted(
            "The omp plugin lets /schwitzer plan, build and propose this project's tasks in \
             that repo."
        ),
    ]
    .spacing(SP_2);

    let agents = column![
        heading("Your agents"),
        switch_row(
            "commit-trailer-toggle",
            Switch::Flat,
            "Task number in commits",
            "omp holds back a commit until its message carries Schwitzer-Task: #<number>. Only \
             your own agents.",
            app.selected
                .is_some_and(|room| app.settings.commit_trailer(&room, project.id)),
            Message::CommitTrailers,
        ),
    ]
    .spacing(SP_2);

    let name = app.forms.project_name.trim();
    let renamed = !name.is_empty() && name != project.name;
    let rename = column![
        heading("Name"),
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
    ]
    .spacing(SP_2);

    let tasks = app.selected_state().map_or(0, |s| {
        s.tasks.iter().filter(|t| t.project == project.id).count()
    });
    let mut delete = column![heading("Delete project")].spacing(SP_2);
    if tasks > 0 {
        let count = if tasks == 1 {
            "1 task".to_owned()
        } else {
            format!("{tasks} tasks")
        };
        delete = delete.push(muted(format!(
            "Only an empty project can be deleted. Move its {count} into another project first."
        )));
    } else {
        delete = delete.push(muted(
            "Deleting removes the project for everyone in the room.",
        ));
    }
    delete = delete.push(
        if app.confirm == Some(Confirm::DeleteProject) && tasks == 0 {
            confirm_row(
                format!("Delete {}?", project.name),
                "confirm-delete-project",
                Icon::Trash,
                "Delete",
                Message::DeleteProject,
            )
        } else {
            icon_text_button(
                "delete-project",
                Icon::Trash,
                Tone::Danger,
                "Delete project",
                (tasks == 0).then_some(Message::Ask(Confirm::DeleteProject)),
                theme::button_danger,
            )
        },
    );

    let body = column![
        section(repos_section),
        section(agents),
        section(rename),
        section(delete),
    ]
    .spacing(SP_4);
    frame(
        "project-settings-page",
        project.name.as_str(),
        body.into(),
        None,
    )
}
