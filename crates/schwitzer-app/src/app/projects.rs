//! Rooms and projects: creating and joining a room, creating, renaming and deleting a project,
//! the projects folders and the repos linked to a project. The pages are `settings_view.rs`.

use std::path::PathBuf;

use iced::Task;
use schwitzer_proto::{Request, Response, RoomId, RoomKey, ServerAddr, caps};

use super::{App, FolderFor, Message, Page, logic};
use crate::bridge::call;
use crate::settings;

impl App {
    pub(super) fn submit_create(&mut self) -> Task<Message> {
        let server = match self.forms.server_url.trim().parse::<ServerAddr>() {
            Ok(server) => server,
            Err(e) => {
                self.banner = Some(format!("Server address: {e}"));
                return Task::none();
            }
        };
        let name = self.forms.room_name.trim();
        if name.is_empty() {
            self.banner = Some("Room name is empty".to_owned());
            return Task::none();
        }
        let name = caps::cut(name, caps::ROOM_NAME).to_owned();
        let client = self.client.clone();
        call(
            async move { client.create_room(server, name).await },
            Message::Created,
        )
    }

    /// Lists the rooms anew and opens `room`, a room just created or joined, on the main page.
    fn enter_room(&mut self, room: RoomId) -> Task<Message> {
        self.sync_rooms();
        self.select(Some(room));
        self.save_settings();
        Task::batch([self.publish(), self.open_page(Page::Main)])
    }

    pub(super) fn on_created(&mut self, result: Result<RoomKey, String>) -> Task<Message> {
        match result {
            Ok(key) => {
                self.settings.last_server = Some(key.server.to_string());
                self.forms.room_name.clear();
                self.enter_room(RoomId::of(&key.token))
            }
            Err(e) => {
                self.banner = Some(e);
                Task::none()
            }
        }
    }

    pub(super) fn submit_join(&mut self) -> Task<Message> {
        let key = match self.forms.room_key.parse::<RoomKey>() {
            Ok(key) => key,
            Err(e) => {
                self.banner = Some(format!("Room key: {e}"));
                return Task::none();
            }
        };
        let client = self.client.clone();
        call(async move { client.join_room(key).await }, Message::Joined)
    }

    pub(super) fn on_joined(&mut self, result: Result<RoomId, String>) -> Task<Message> {
        match result {
            Ok(room) => {
                self.forms.room_key.clear();
                self.enter_room(room)
            }
            Err(e) => {
                self.banner = Some(e);
                Task::none()
            }
        }
    }

    pub(super) fn create_project(&self) -> Task<Message> {
        let name = caps::clamp(&self.forms.new_project, caps::PROJECT_NAME).to_owned();
        let Some(room) = self.selected.filter(|_| !name.is_empty()) else {
            return Task::none();
        };
        call(
            self.request(room, Request::CreateProject { room, name }),
            move |result| Message::ProjectCreated(room, result),
        )
    }

    pub(super) fn project_created(
        &mut self,
        room: RoomId,
        result: Result<Response, String>,
    ) -> Task<Message> {
        match result {
            Ok(Response::Project(project)) => {
                self.forms.new_project.clear();
                self.choose_project(room, project.id);
                if self.selected == Some(room) && self.page != Page::FirstStart {
                    self.page = Page::Main;
                    self.close_task();
                }
                self.sync_project_form();
                self.read_project_dirs();
                self.notify("Project created");
            }
            Ok(other) => self.banner = Some(logic::unexpected(&other)),
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    pub(super) fn rename_project(&self) -> Task<Message> {
        let name = caps::clamp(&self.forms.project_name, caps::PROJECT_NAME).to_owned();
        let (Some(room), Some(project)) = (self.selected, self.selected_project()) else {
            return Task::none();
        };
        if name.is_empty() || name == project.name {
            return Task::none();
        }
        let req = Request::RenameProject {
            room,
            project: project.id,
            name,
        };
        call(self.request(room, req), Message::ProjectRenamed)
    }

    pub(super) fn project_renamed(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(Response::Project(project)) => {
                // The rename event may come after the reply: show the new name now.
                self.forms.project_name = project.name;
                self.notify("Project renamed");
            }
            Ok(other) => self.banner = Some(logic::unexpected(&other)),
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    pub(super) fn delete_project(&mut self) -> Task<Message> {
        self.confirm = None;
        let (Some(room), Some(project)) = (self.selected, self.selected_project()) else {
            return Task::none();
        };
        let req = Request::DeleteProject {
            room,
            project: project.id,
        };
        call(self.request(room, req), Message::ProjectDeleted)
    }

    pub(super) fn project_deleted(&mut self, result: Result<Response, String>) -> Task<Message> {
        match result {
            Ok(Response::Ok) => {
                self.notify("Project deleted");
                if self.page == Page::ProjectSettings {
                    return self.open_page(Page::RoomSettings);
                }
            }
            Ok(other) => self.banner = Some(logic::unexpected(&other)),
            Err(e) => self.banner = Some(e),
        }
        Task::none()
    }

    /// Links the folder typed into the repo field to the selected project.
    pub(super) fn add_repo(&mut self) -> Task<Message> {
        match self.link_repo(settings::expand_home(self.forms.repo.trim())) {
            Some(task) => {
                self.forms.repo.clear();
                task
            }
            None => Task::none(),
        }
    }

    /// Links `path` to the selected project; `None` when nothing was linked.
    pub(super) fn link_repo(&mut self, path: PathBuf) -> Option<Task<Message>> {
        let room = self.selected?;
        let project = self.selected_project()?.id;
        let path = match logic::existing_folder(path) {
            Ok(path) => path,
            Err(banner) => {
                self.banner = Some(banner);
                return None;
            }
        };
        let from_main = self.page == Page::Main;
        self.settings.link_repo(&room, project, path.clone());
        self.save_settings();
        // Reads whether the repo carries the plugin already, so the prompt and the agent dot
        // are right at once.
        self.check_plugins();
        self.plugin_prompt = (from_main && !self.plugins.contains_key(&path)).then_some(path);
        Some(self.publish())
    }

    pub(super) fn remove_repo(&mut self, index: usize) -> Task<Message> {
        let (Some(room), Some(project)) = (self.selected, self.selected_project()) else {
            return Task::none();
        };
        self.settings.unlink_repo(&room, project.id, index);
        self.save_settings();
        self.publish()
    }

    /// Adds the folder typed into the projects folder field; the field is cleared when it was
    /// added.
    pub(super) fn submit_projects_dir(&mut self) {
        let input = self.forms.new_projects_dir.trim();
        if input.is_empty() {
            return;
        }
        if self.add_projects_dir(settings::expand_home(input)) {
            self.forms.new_projects_dir.clear();
        }
    }

    /// Adds `path` to the projects folders; `false` when it is not a folder on this machine
    /// (with a banner). A folder already listed counts as added.
    pub(super) fn add_projects_dir(&mut self, path: PathBuf) -> bool {
        let path = match logic::existing_folder(path) {
            Ok(path) => path,
            Err(banner) => {
                self.banner = Some(banner);
                return false;
            }
        };
        let dirs = self.settings.projects_dirs.get_or_insert_with(Vec::new);
        if dirs.contains(&path) {
            return true;
        }
        dirs.push(path);
        self.save_settings();
        self.read_project_dirs();
        self.notify("Projects folder added");
        true
    }

    pub(super) fn remove_projects_dir(&mut self, index: usize) {
        // An emptied list stays `Some`, so the defaults are not entered again.
        let Some(dirs) = self
            .settings
            .projects_dirs
            .as_mut()
            .filter(|dirs| index < dirs.len())
        else {
            return;
        };
        dirs.remove(index);
        self.save_settings();
        self.read_project_dirs();
        self.notify("Projects folder removed");
    }

    /// Reads the subfolders of the searched folders.
    pub(super) fn read_project_dirs(&mut self) {
        let mut dirs: Vec<PathBuf> = self
            .searched_dirs()
            .into_iter()
            .flat_map(settings::subdirs)
            .collect();
        dirs.sort();
        dirs.dedup();
        self.project_dirs = dirs;
    }

    /// Opens the folder dialog for `for_`, unless one is open.
    pub(super) fn open_folder_dialog(&mut self, for_: FolderFor) -> Task<Message> {
        if self.folder_dialog {
            return Task::none();
        }
        self.folder_dialog = true;
        let (title, typed) = match for_ {
            FolderFor::Repo => ("Choose a repo", self.forms.repo.trim()),
            FolderFor::ProjectsDir => (
                "Choose a projects folder",
                self.forms.new_projects_dir.trim(),
            ),
        };
        let typed = settings::expand_home(typed);
        let start = if typed.is_dir() {
            Some(typed)
        } else {
            match for_ {
                FolderFor::Repo => self.searched_dirs().first().map(|p| p.to_path_buf()),
                FolderFor::ProjectsDir => settings::home_dir(),
            }
        };
        Task::perform(choose_folder(title, start), move |path| {
            Message::FolderChosen(for_, path)
        })
    }

    /// The folder dialog closed with `path`, `None` when cancelled.
    pub(super) fn folder_chosen(
        &mut self,
        for_: FolderFor,
        path: Option<PathBuf>,
    ) -> Task<Message> {
        self.folder_dialog = false;
        let Some(path) = path else {
            return Task::none();
        };
        match for_ {
            FolderFor::Repo => self.link_repo(path).unwrap_or_else(Task::none),
            FolderFor::ProjectsDir => {
                self.add_projects_dir(path);
                Task::none()
            }
        }
    }

    /// Leaves the selected room on its server.
    pub(super) fn leave_room(&mut self) -> Task<Message> {
        self.confirm = None;
        match self.selected {
            Some(room) => {
                let client = self.client.clone();
                call(async move { client.leave_room(room).await }, Message::Left)
            }
            None => Task::none(),
        }
    }

    /// The server answered the leave of the selected room.
    pub(super) fn room_left(&mut self, result: Result<(), String>) -> Task<Message> {
        match result {
            Ok(()) => {
                self.sync_rooms();
                self.notify("Room left");
                self.leave_room_page()
            }
            Err(e) => {
                self.banner = Some(e);
                Task::none()
            }
        }
    }

    /// Forgets the selected room in this app, without telling its server.
    pub(super) fn remove_room(&mut self) -> Task<Message> {
        self.confirm = None;
        let Some(room) = self.selected else {
            return Task::none();
        };
        self.client.forget_room(room);
        self.sync_rooms();
        self.notify("Room removed from this app");
        self.leave_room_page()
    }

    /// Sets whether the selected project's commits carry the Schwitzer trailers.
    pub(super) fn set_commit_trailers(&mut self, on: bool) -> Task<Message> {
        if let Some((room, project)) = self.selected_ids() {
            self.settings.set_commit_trailer(&room, project, on);
            self.save_settings();
            self.write_plugin_configs(&room, project);
        }
        Task::none()
    }
}

/// Env var a headless instance answers the folder dialog with (it never opens one).
#[cfg(feature = "remote")]
const PICK_FOLDER_ENV: &str = "SCHWITZER_PICK_FOLDER";

/// Asks the user for a folder, starting in `start`; `None` when cancelled. A headless
/// instance opens no dialog: it answers with `$SCHWITZER_PICK_FOLDER`, or `None` when unset.
async fn choose_folder(title: &'static str, start: Option<PathBuf>) -> Option<PathBuf> {
    #[cfg(feature = "remote")]
    if iced_impeccable::is_headless() {
        return std::env::var_os(PICK_FOLDER_ENV)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
    }
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    if let Some(start) = start {
        dialog = dialog.set_directory(start);
    }
    dialog.pick_folder().await.map(|f| f.path().to_path_buf())
}
