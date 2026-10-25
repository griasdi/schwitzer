//! The main page below the header bar: the task list and sessions, or the empty state that
//! says what is missing (a room, a project, a linked repo, the omp plugin).

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};
use schwitzer_proto::Project;

use super::icons::{Icon, Tone, icon};
use super::widgets::{hrule, icon_button, icon_text_button, input, muted, path_text, text_button};
use super::{App, FolderFor, Message, Page, list, live, logic};
use crate::theme::{self, SP_2};

impl App {
    /// The main page below the header bar: list and sessions, or an empty state.
    pub(super) fn main(&self) -> Element<'_, Message> {
        match (self.selected_state(), self.selected) {
            (Some(state), _) => match self.selected_project() {
                None => self.no_projects(),
                Some(project) if self.shows_list() => column![
                    container(list::view(self, state, project)).height(Length::Fill),
                    hrule(),
                    list::idea_bar(self),
                    hrule(),
                    live::view(self, state, project.id),
                ]
                .into(),
                Some(project) if self.project_repos().is_empty() => self.link_view(project),
                Some(project) => self.plugin_view(project),
            },
            (None, Some(_)) => {
                let (title, hint) = match self.selected_key() {
                    Some(key) if self.selected_server_online() == Some(false) => (
                        "Server not reachable",
                        format!(
                            "{} does not answer. If it is gone for good, remove the room in Settings.",
                            key.server.authority()
                        ),
                    ),
                    _ => (
                        "Connecting…",
                        "The room shows up as soon as its server answers.".to_owned(),
                    ),
                };
                list::empty(
                    Icon::Plug,
                    title,
                    hint,
                    icon_text_button(
                        "empty-settings",
                        Icon::Settings,
                        Tone::Text,
                        "Settings",
                        Some(Message::OpenPage(Page::RoomSettings)),
                        theme::button_secondary,
                    ),
                )
            }
            (None, None) => list::empty(
                Icon::Users,
                "Welcome to Schwitzer",
                "Create a room on a server, or join one with the key someone shared.",
                icon_text_button(
                    "empty-rooms",
                    Icon::Users,
                    Tone::OnPrimary,
                    "Create or join a room",
                    Some(Message::OpenPage(Page::Rooms)),
                    theme::button_primary,
                ),
            ),
        }
    }

    /// The selected room has no project yet: create the first one.
    fn no_projects(&self) -> Element<'_, Message> {
        let filled = !self.forms.new_project.trim().is_empty();
        list::empty(
            Icon::FolderPlus,
            "No projects yet",
            "A project holds a part of the room's tasks, for example one repo. Create the first \
             one.",
            row![
                input(
                    "new-project-name",
                    "Project name",
                    &self.forms.new_project,
                    Message::NewProjectName,
                    Message::CreateProject,
                ),
                text_button(
                    "create-project",
                    "Create",
                    filled.then_some(Message::CreateProject),
                    theme::button_primary,
                ),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center)
            .into(),
        )
    }

    /// A repo was just linked from the main page: offer to install the omp plugin into it.
    fn plugin_view<'a>(&'a self, project: &'a Project) -> Element<'a, Message> {
        let index = self.prompted_plugin().unwrap_or_default();
        let repo = self
            .project_repos()
            .get(index)
            .map(|repo| repo.display().to_string())
            .unwrap_or_default();
        list::empty(
            Icon::Plug,
            format!("Set up omp in {}", project.name),
            "omp agents started in this repo reach Schwitzer through the plugin: /schwitzer \
             plan, build, propose and idea.",
            column![
                row![icon(Icon::Folder, 16.0, Tone::Muted), path_text(&repo)]
                    .spacing(SP_2)
                    .align_y(Alignment::Center),
                row![
                    text_button(
                        "plugin-prompt-skip",
                        "Not now",
                        Some(Message::SkipPluginPrompt),
                        theme::button_ghost,
                    ),
                    text_button(
                        "plugin-prompt-install",
                        "Install plugin",
                        Some(Message::InstallPlugin(index)),
                        theme::button_primary,
                    ),
                ]
                .spacing(SP_2),
            ]
            .spacing(SP_2)
            .align_x(Alignment::Center)
            .into(),
        )
    }

    /// The selected project has no repo on this machine yet: link one, from the suggestions
    /// out of the projects folders or by path.
    fn link_view<'a>(&'a self, project: &'a Project) -> Element<'a, Message> {
        let suggested =
            logic::suggested_repos(&project.name, &self.project_dirs, self.project_repos());
        let mut col = column![].spacing(SP_2).width(Length::Fill);
        for (index, path) in suggested.iter().enumerate() {
            col = col.push(
                row![
                    icon(Icon::Folder, 16.0, Tone::Muted),
                    path_text(&path.display().to_string()),
                    text_button(
                        format!("link-suggested-{index}"),
                        "Link",
                        Some(Message::LinkRepo((*path).clone())),
                        theme::button_primary,
                    ),
                ]
                .spacing(SP_2)
                .align_y(Alignment::Center),
            );
        }
        let filled = !self.forms.repo.trim().is_empty();
        col = col.push(
            row![
                input(
                    "repo-input",
                    "/path/to/repo",
                    &self.forms.repo,
                    Message::RepoInput,
                    Message::AddRepo,
                ),
                icon_button(
                    "repo-browse",
                    Icon::FolderOpen,
                    Tone::Muted,
                    "Choose a folder",
                    (!self.folder_dialog).then_some(Message::ChooseFolder(FolderFor::Repo)),
                    theme::button_secondary,
                ),
                text_button(
                    "repo-add",
                    "Link",
                    filled.then_some(Message::AddRepo),
                    if suggested.is_empty() {
                        theme::button_primary
                    } else {
                        theme::button_secondary
                    },
                ),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
        );
        let searched = self.searched_dirs();
        let hint = if searched.is_empty() {
            "No projects folder yet. Add one to get matching repos suggested.".to_owned()
        } else {
            let names: Vec<String> = searched
                .iter()
                .map(|dir| logic::breakable_path(&logic::folder_label(dir)))
                .collect();
            format!("Searched in {}.", names.join(", "))
        };
        col = col.push(
            row![
                muted(hint)
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
                text_button(
                    "change-projects-dirs",
                    "Change",
                    Some(Message::OpenProjectsDirs),
                    theme::button_ghost,
                ),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
        );
        list::empty(
            Icon::FolderPlus,
            format!("Link a repo to {}", project.name),
            "Its tasks show up once a repo on this machine is linked to it. omp sessions \
             running in linked repos are shared with the room.",
            col.into(),
        )
    }
}
