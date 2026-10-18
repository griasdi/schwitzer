//! The application: state, messages and `update`. The views live in the submodules.

mod chrome;
mod icons;
mod licenses_view;
mod list;
mod live;
pub mod logic;
mod pages;
mod resume;
mod settings_view;
mod task_view;
mod widgets;

pub use self::resume::Resumed;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::operation::AbsoluteOffset;
use iced::widget::selector::{self, Target};
use iced::widget::{column, container, markdown, operation, text_editor};
use iced::{Element, Length, Subscription, Task, Theme};
use schwitzer_client::{Client, ClientError, ClientEvent};
use schwitzer_proto::{
    Decision, Priority, Project, Request, Response, RoomId, RoomKey, RoomState, ServerAddr,
    SessionInfo, Stage, Task as RoomTask, Work, caps, now_ms,
};
use serde::{Deserialize, Serialize};

use self::chrome::RoomChoice;
use self::icons::{Icon, Tone};
use self::logic::Group;
use crate::bridge::{self, ClientKey, call, fire};
use crate::mcp::{self, McpStatus};
use crate::omp_plugin::{self, PluginState};
use crate::sessions::{self, LocalSession};
use crate::settings::{self, Settings};
use crate::theme::{self, SP_2};
use crate::update::{self, UpdateState};
use crate::wsl;

/// The agent name a pick claims the build with; the picker's own agent finishes it.
const PICK_AGENT: &str = "omp";

/// How long a status-bar notice stays.
const NOTICE_MS: u64 = 5_000;
/// How often the omp plugins in the shared repos are compared with this app's.
const PLUGIN_CHECK_MS: u64 = 5_000;

/// Everything the user and the outside world can make the app do.
#[derive(Debug, Clone)]
pub enum Message {
    /// The one-second clock behind the runtimes.
    Tick,
    /// The system switched between light and dark.
    SystemMode(iced::theme::Mode),
    Client(ClientEvent),
    /// The event stream lagged: reload the rooms from the client.
    Resync,
    LocalSessions(Vec<LocalSession>),
    /// The running WSL distros changed.
    Wsl(Vec<wsl::Distro>),
    Mcp(McpStatus),
    Noop,
    /// A link in task text was clicked.
    OpenLink(markdown::Uri),
    /// A call failed; the text goes into the banner.
    Failed(String),
    DismissBanner,
    FocusNext,
    FocusPrevious,

    /// Opens the draft of a new task at the top of the Proposals group, or of the Ideas group
    /// for an idea.
    OpenNewTask {
        idea: bool,
    },
    OpenSettings,
    /// Opens the Licenses page from the About section of the settings.
    OpenLicenses,
    /// Unfolds license `i` of `about::entries()` on the Licenses page, or folds it.
    ToggleLicense(usize),
    /// Opens task `n` in place in the list and scrolls it into view (the `#n` links in
    /// sessions).
    OpenTask(u64),
    /// A press on a task slab or on the head of the open task: opens it, or closes it when it
    /// is open.
    ToggleTask(u64),
    /// Esc, the back button, the close button of the open task and Cancel on a draft: closes
    /// the open task or draft, else returns from Settings to the main page.
    Back,
    ToggleGroup(Group),
    /// Folds or unfolds the sessions panel.
    ToggleSessions,
    /// Folds or unfolds the member list under the header bar.
    ToggleMembers,

    SelectRoom(RoomId),
    ServerUrl(String),
    RoomName(String),
    RoomKeyInput(String),
    SubmitCreate,
    Created(Result<RoomKey, String>),
    SubmitJoin,
    Joined(Result<RoomId, String>),
    CopyKey,
    LeaveRoom,
    Left(Result<(), String>),
    /// Forgets the selected room on this machine without asking its server, which does not
    /// answer.
    RemoveRoom,
    NameInput(String),
    SubmitName,
    /// Shows project `id` of the selected room.
    SelectProject(u64),
    /// Opens Settings with the focus on the new project's name.
    OpenNewProject,
    NewProjectName(String),
    CreateProject,
    ProjectCreated(RoomId, Result<Response, String>),
    ProjectNameInput(String),
    RenameProject,
    ProjectRenamed(Result<Response, String>),
    /// Moves the open task into project `id`.
    MoveTask(u64),
    TaskMoved(Result<Response, String>),
    ProjectsDirInput(String),
    AddProjectsDir,
    /// Index into the projects folders.
    RemoveProjectsDir(usize),
    /// Opens Settings with the focus on the projects folder input.
    OpenProjectsDirs,
    RepoInput(String),
    AddRepo,
    /// Links a suggested folder to the selected project.
    LinkRepo(PathBuf),
    /// Opens the folder dialog.
    ChooseFolder(FolderFor),
    /// The folder dialog closed; `None` when cancelled.
    FolderChosen(FolderFor, Option<PathBuf>),
    RemoveRepo(usize),
    /// Index into the selected project's repos.
    InstallPlugin(usize),
    /// Index into the selected project's repos.
    RemovePlugin(usize),
    /// Index into the selected project's repos.
    UpdatePlugin(usize),
    UpdateAllPlugins,
    AutoUpdatePlugins(bool),

    ProposalTitle(String),
    ProposalBody(text_editor::Action),
    Propose,
    Proposed(Result<Response, String>),
    Pick(u64),
    Picked(Result<(String, RoomTask), String>),

    EditTask,
    CancelEditTask,
    EditTitle(String),
    EditBody(text_editor::Action),
    SaveTask,
    ProposeIdea,
    TaskSaved(Result<Response, String>),
    EditPlan,
    CancelEditPlan,
    PlanAction(text_editor::Action),
    SavePlan,
    PlanSaved(Result<Response, String>),
    SetPriority(Priority),
    Decide(u64, Decision),
    Release(u64),
    NoteInput(String),
    AddNote,
    NoteAdded(Result<Response, String>),
    Published(RoomId, Result<(), String>),
    /// The five-minute update check and "Check now"; a server's announcement checks too.
    CheckUpdate,
    UpdateChecked(Result<Option<update::Release>, String>),
    InstallUpdate,
    UpdateInstalled(Result<semver::Version, String>),
    /// Restarts into the installed update; first asks which widget has the focus.
    RestartForUpdate,
    /// The focused widget and the window's place just before the restart.
    Restart(Option<iced::advanced::widget::Id>, Option<Placement>),
    AutoInstallUpdates(bool),
    AutoRestartUpdates(bool),
    /// Whether the element to focus after a restart is laid out yet.
    ResumeFocus(bool),
}

/// Whether a server answers, with the reason when it does not.
#[derive(Debug, Clone, Default)]
struct ServerStatus {
    online: bool,
    error: Option<String>,
}

/// Which page fills the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Page {
    Main,
    Settings,
    /// Schwitzer's license and the open source licenses, opened from Settings.
    Licenses,
}

/// The one element open in place in the task list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Expanded {
    /// Task `n` shown in full: details, people, decisions, plan, priority, notes.
    Task(u64),
    /// The draft of a new proposal, or of an idea, at the top of its group.
    New { idea: bool },
}

/// A short confirmation in the status bar.
struct Notice {
    text: String,
    until_ms: u64,
}

/// What a chosen folder is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderFor {
    /// Linked to the selected project.
    Repo,
    /// Added to the projects folders.
    ProjectsDir,
}

/// The inputs of the settings page, the draft of a new task and the note field of the open
/// task.
struct Forms {
    server_url: String,
    room_name: String,
    room_key: String,
    name: String,
    repo: String,
    new_project: String,
    /// The rename input of the selected project.
    project_name: String,
    new_projects_dir: String,
    proposal_title: String,
    proposal_body: text_editor::Content,
    note: String,
}

/// The unsaved title and body of the task being edited.
struct TaskDraft {
    title: String,
    body: text_editor::Content,
}

/// Markdown parsed from `src`; parsed again only when the text changes.
#[derive(Debug, Default)]
struct ParsedMarkdown {
    src: String,
    content: markdown::Content,
}

impl ParsedMarkdown {
    fn sync(&mut self, src: &str) {
        if src != self.src {
            self.src = src.to_owned();
            self.content = markdown::Content::parse(&self.src);
        }
    }

    fn items(&self) -> &[markdown::Item] {
        self.content.items()
    }
}

/// The open task's details, plan and notes (in note order) as Markdown.
#[derive(Debug, Default)]
struct OpenMarkdown {
    body: ParsedMarkdown,
    plan: ParsedMarkdown,
    notes: Vec<ParsedMarkdown>,
}

impl OpenMarkdown {
    /// Follows `task`; no open task leaves everything empty.
    fn sync(&mut self, task: Option<&RoomTask>) {
        let (body, plan, notes) = task.map_or(("", "", &[][..]), |t| {
            (t.body.as_str(), t.plan.as_str(), t.notes.as_slice())
        });
        self.body.sync(body);
        self.plan.sync(plan);
        self.notes.truncate(notes.len());
        self.notes.resize_with(notes.len(), ParsedMarkdown::default);
        for (md, note) in self.notes.iter_mut().zip(notes) {
            md.sync(&note.text);
        }
    }
}

/// The element to focus or reveal once the restored page shows it.
struct PendingFocus {
    focus: Option<&'static str>,
    until_ms: u64,
}

/// Where the window was on screen, in logical pixels: outer position (`None` where the
/// platform does not tell, as on Wayland), inner size and whether it was maximized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub position: Option<(f32, f32)>,
    pub size: (f32, f32),
    pub maximized: bool,
}

pub struct App {
    client: Client,
    data_dir: PathBuf,
    settings: Settings,
    /// The settings as last written (or read), to save only real changes.
    saved: Settings,
    theme: Theme,
    /// The joined rooms in list order, with their last known state.
    rooms: Vec<(RoomKey, Option<RoomState>)>,
    servers: HashMap<ServerAddr, ServerStatus>,
    selected: Option<RoomId>,
    /// The project last picked per room; in memory only.
    chosen_projects: HashMap<RoomId, u64>,
    /// The subfolders of the projects folder, for suggestions; read outside `view`.
    project_dirs: Vec<PathBuf>,
    page: Page,
    /// The unfolded entry of the Licenses page.
    license_open: Option<usize>,
    /// The task or draft open in place in the list.
    expanded: Option<Expanded>,
    /// Folded groups of the list.
    collapsed: HashSet<Group>,
    /// Whether the sessions panel is unfolded.
    sessions_open: bool,
    /// Whether the member list under the header bar is unfolded.
    members_open: bool,
    /// The task whose pick is under way.
    picking: Option<u64>,
    /// Whether a folder dialog is open; its buttons are disabled meanwhile, so a second click
    /// opens no second dialog.
    folder_dialog: bool,
    forms: Forms,
    task_draft: Option<TaskDraft>,
    plan_draft: Option<text_editor::Content>,
    /// Details, plan and notes of the open task, parsed as Markdown.
    open_md: OpenMarkdown,
    banner: Option<String>,
    notice: Option<Notice>,
    local_sessions: Vec<LocalSession>,
    /// The running WSL distros (Windows only): their projects folders are searched, their omp
    /// sessions read and their linked repos checked.
    wsl: Vec<wsl::Distro>,
    /// What was last published to each room; a room missing here has had nothing published.
    published: HashMap<RoomId, Vec<SessionInfo>>,
    mcp: Option<McpStatus>,
    mcp_token: Option<String>,
    /// Shared repos with the omp plugin installed, and how it compares with this app's.
    plugins: BTreeMap<PathBuf, PluginState>,
    /// Repos whose automatic update failed; not retried until their plugin changes state or
    /// the user updates by hand, so a broken repo does not raise the banner every check.
    auto_failed: BTreeSet<PathBuf>,
    /// When the plugins were last checked.
    plugins_checked_ms: u64,
    /// This program's file as it was at start (the AppImage when it runs from one); the restart
    /// after an update runs it. (On Linux, `current_exe` reads `… (deleted)` once the file is
    /// replaced.)
    program: Option<update::Program>,
    updater: UpdateState,
    /// The version whose install failed and why, so the periodic check does not download it
    /// again by itself.
    update_failed: Option<(semver::Version, String)>,
    /// The release each online server announced: the newest stable one on GitHub.
    releases: HashMap<ServerAddr, semver::Version>,
    /// What to focus or reveal after a restart for an update, once it shows.
    resume_focus: Option<PendingFocus>,
    now_ms: u64,
}

impl App {
    pub fn boot(resumed: &Resumed) -> (Self, Task<Message>) {
        let data_dir = settings::data_dir();
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            fatal(&format!("cannot create {}: {e}", data_dir.display()));
        }
        let (mut settings, problem) = Settings::load(&data_dir);
        let unreadable = problem.is_some();
        let saved_before = settings.clone();
        if settings.projects_dirs.is_none() {
            settings.projects_dirs = Some(
                directories::BaseDirs::new()
                    .map(|dirs| settings::default_projects_dirs(dirs.home_dir()))
                    .unwrap_or_default(),
            );
        }
        let client = Client::start(&settings::identity_dir(&data_dir), settings.name.clone())
            .unwrap_or_else(|e| fatal(&format!("cannot start the network client: {e}")));

        let mut notices = Vec::new();
        if let Some(problem) = problem {
            notices.push(format!("Settings not read, using defaults: {problem}"));
        }
        for key in &settings.rooms {
            match key.parse::<RoomKey>() {
                Ok(key) => client.add_room(key),
                Err(e) => notices.push(format!("Ignored room key in settings ({e}): {key}")),
            }
        }
        let (mcp, mcp_token) = match mcp::load_or_create_token(&data_dir) {
            Ok(token) => (None, Some(token)),
            Err(e) => (
                Some(McpStatus::Failed(format!("no access token: {e}"))),
                None,
            ),
        };

        let rooms = client.rooms();
        let selected = rooms.first().map(|(key, _)| RoomId::of(&key.token));
        let mut app = Self {
            forms: Forms {
                server_url: settings.last_server.clone().unwrap_or_default(),
                room_name: String::new(),
                room_key: String::new(),
                name: settings.name.clone(),
                repo: String::new(),
                new_project: String::new(),
                project_name: String::new(),
                new_projects_dir: String::new(),
                proposal_title: String::new(),
                proposal_body: text_editor::Content::new(),
                note: String::new(),
            },
            project_dirs: Vec::new(),
            // An unreadable file is never overwritten at start; otherwise the first-start
            // projects folders are saved below.
            saved: if unreadable {
                settings.clone()
            } else {
                saved_before
            },
            settings,
            theme: theme::for_mode(iced::theme::Mode::Dark),
            client,
            data_dir,
            rooms,
            servers: HashMap::new(),
            selected,
            chosen_projects: HashMap::new(),
            page: Page::Main,
            license_open: None,
            expanded: None,
            collapsed: HashSet::from([Group::Closed]),
            sessions_open: true,
            members_open: false,
            picking: None,
            folder_dialog: false,
            task_draft: None,
            plan_draft: None,
            open_md: OpenMarkdown::default(),
            banner: (!notices.is_empty()).then(|| notices.join("\n")),
            notice: None,
            local_sessions: Vec::new(),
            wsl: Vec::new(),
            published: HashMap::new(),
            mcp,
            mcp_token,
            plugins: BTreeMap::new(),
            auto_failed: BTreeSet::new(),
            plugins_checked_ms: 0,
            program: update::Program::current(),
            updater: UpdateState::Idle,
            update_failed: None,
            releases: HashMap::new(),
            resume_focus: None,
            now_ms: now_ms(),
        };
        app.read_project_dirs();
        if update::ENABLED
            && let Some(program) = &app.program
        {
            update::clean_leftovers(&program.path);
        }
        if let Some(resume) = resumed.0.clone() {
            app.apply_resume(resume);
        }
        // Finds out-of-date plugins; with auto-update on, this refreshes them after an app
        // update or a new MCP port.
        app.check_plugins();
        app.save_settings();
        // The first `ClientEvent::Release` or the first periodic check (GitHub's latest release
        // when no server announced one) starts the update check.
        (app, iced::system::theme().map(Message::SystemMode))
    }

    pub fn title(&self) -> String {
        match self.selected_state() {
            Some(state) => format!("{} · Schwitzer", state.name),
            None => "Schwitzer".to_owned(),
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            Subscription::run_with(ClientKey(self.client.clone()), bridge::client_events),
            iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
            iced::system::theme_changes().map(Message::SystemMode),
            keyboard::listen().filter_map(key_message),
            sessions::subscription(self.sessions_dirs()).map(Message::LocalSessions),
            wsl::subscription().map(Message::Wsl),
        ];
        if let Some(token) = &self.mcp_token {
            subs.push(
                mcp::subscription(self.client.clone(), self.settings.mcp_port, token.clone())
                    .map(Message::Mcp),
            );
        }
        if update::ENABLED {
            subs.push(iced::time::every(update::CHECK_INTERVAL).map(|_| Message::CheckUpdate));
        }
        Subscription::batch(subs)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut page = column![];
        if let Some(banner) = &self.banner {
            page = page.push(container(widgets::banner(banner)).padding(SP_2));
        }
        let body: Element<'_, Message> = match self.page {
            Page::Main => {
                let mut main = column![chrome::header(self), widgets::hrule()];
                if let Some(members) = chrome::members(self) {
                    main = main.push(members).push(widgets::hrule());
                }
                main.push(self.main()).into()
            }
            Page::Settings => settings_view::view(self),
            Page::Licenses => licenses_view::view(self),
        };
        container(
            page.push(container(body).height(Length::Fill))
                .push(widgets::hrule())
                .push(chrome::status_bar(self)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::ground)
        .into()
    }

    /// The main page below the header bar: list and sessions, or an empty state.
    fn main(&self) -> Element<'_, Message> {
        match (self.selected_state(), self.selected) {
            (Some(state), _) => match self.selected_project() {
                None => self.no_projects(),
                Some(project) if self.project_repos().is_empty() => self.link_view(project),
                Some(project) => column![
                    container(list::view(self, state, project)).height(Length::Fill),
                    widgets::hrule(),
                    live::view(self, state, project.id),
                ]
                .into(),
            },
            (None, Some(_)) => {
                let (title, hint) = match self.selected_key() {
                    Some(key) if self.selected_server_online() == Some(false) => (
                        "Server not reachable",
                        format!(
                            "{}:{} does not answer. If it is gone for good, remove the room in Settings.",
                            key.server.host, key.server.port
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
                    widgets::icon_text_button(
                        "empty-settings",
                        Icon::Settings,
                        Tone::Text,
                        "Settings",
                        Some(Message::OpenSettings),
                        theme::button_secondary,
                    ),
                )
            }
            (None, None) => list::empty(
                Icon::Users,
                "Welcome to Schwitzer",
                "Create a room on a server, or join one with the key someone shared.",
                widgets::icon_text_button(
                    "empty-rooms",
                    Icon::Users,
                    Tone::OnPrimary,
                    "Create or join a room",
                    Some(Message::OpenSettings),
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
            iced::widget::row![
                widgets::input(
                    "new-project-name",
                    "Project name",
                    &self.forms.new_project,
                    Message::NewProjectName,
                    Message::CreateProject,
                ),
                widgets::text_button(
                    "create-project",
                    "Create",
                    filled.then_some(Message::CreateProject),
                    theme::button_primary,
                ),
            ]
            .spacing(SP_2)
            .align_y(iced::Alignment::Center)
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
                iced::widget::row![
                    icons::icon(Icon::Folder, 16.0, Tone::Muted),
                    widgets::path_text(&path.display().to_string()),
                    widgets::text_button(
                        format!("link-suggested-{index}"),
                        "Link",
                        Some(Message::LinkRepo((*path).clone())),
                        theme::button_primary,
                    ),
                ]
                .spacing(SP_2)
                .align_y(iced::Alignment::Center),
            );
        }
        let filled = !self.forms.repo.trim().is_empty();
        col = col.push(
            iced::widget::row![
                widgets::input(
                    "repo-input",
                    "/path/to/repo",
                    &self.forms.repo,
                    Message::RepoInput,
                    Message::AddRepo,
                ),
                widgets::icon_button(
                    "repo-browse",
                    Icon::FolderOpen,
                    Tone::Muted,
                    "Choose a folder",
                    (!self.folder_dialog).then_some(Message::ChooseFolder(FolderFor::Repo)),
                    theme::button_secondary,
                ),
                widgets::text_button(
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
            .align_y(iced::Alignment::Center),
        );
        let searched = self.searched_dirs();
        let hint = if searched.is_empty() {
            "No projects folder yet. Add one to get matching repos suggested.".to_owned()
        } else {
            let names: Vec<String> = searched
                .iter()
                .map(|dir| logic::breakable_path(&folder_label(dir)))
                .collect();
            format!("Searched in {}.", names.join(", "))
        };
        col = col.push(
            iced::widget::row![
                widgets::muted(hint)
                    .width(Length::Fill)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                widgets::text_button(
                    "change-projects-dirs",
                    "Change",
                    Some(Message::OpenProjectsDirs),
                    theme::button_ghost,
                ),
            ]
            .spacing(SP_2)
            .align_y(iced::Alignment::Center),
        );
        list::empty(
            Icon::FolderPlus,
            format!("Link a repo to {}", project.name),
            "Its tasks show up once a repo on this machine is linked to it. omp sessions \
             running in linked repos are shared with the room.",
            col.into(),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.refresh_markdown();
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.now_ms = now_ms();
                if self
                    .notice
                    .as_ref()
                    .is_some_and(|n| n.until_ms <= self.now_ms)
                {
                    self.notice = None;
                }
                if self.now_ms >= self.plugins_checked_ms + PLUGIN_CHECK_MS {
                    self.check_plugins();
                    self.read_project_dirs();
                }
                self.pending_focus()
            }
            Message::SystemMode(mode) => {
                self.theme = theme::for_mode(mode);
                Task::none()
            }
            Message::Client(event) => self.on_client(event),
            Message::Resync => {
                self.sync_rooms();
                self.publish()
            }
            Message::LocalSessions(sessions) => {
                self.local_sessions = sessions;
                self.publish()
            }
            Message::Wsl(distros) => {
                self.wsl = distros;
                self.enter_wsl_projects_dirs();
                self.read_project_dirs();
                Task::none()
            }
            Message::Mcp(status) => {
                self.mcp = Some(status);
                Task::none()
            }
            Message::Noop => Task::none(),
            Message::OpenLink(url) => {
                self.open_link(&url);
                Task::none()
            }
            Message::Failed(e) => {
                self.banner = Some(e);
                Task::none()
            }
            Message::DismissBanner => {
                self.banner = None;
                Task::none()
            }
            Message::FocusNext => operation::focus_next(),
            Message::FocusPrevious => operation::focus_previous(),
            Message::Published(room, result) => {
                if let Err(e) = result {
                    tracing::debug!(%room, "publishing sessions failed: {e}");
                    self.published.remove(&room);
                }
                Task::none()
            }
            Message::CheckUpdate => self.check_update(),
            Message::UpdateChecked(result) => self.update_checked(result),
            Message::InstallUpdate => match &self.updater {
                UpdateState::Available { release, .. } => {
                    let release = release.clone();
                    self.install_update(release)
                }
                _ => Task::none(),
            },
            Message::UpdateInstalled(result) => self.update_installed(result),
            Message::RestartForUpdate => {
                if !matches!(self.updater, UpdateState::Installed(_)) {
                    return Task::none();
                }
                iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::find_focused(),
                )
                .collect()
                .then(|ids| {
                    let focus = ids.into_iter().next();
                    resume::placement()
                        .map(move |placement| Message::Restart(focus.clone(), placement))
                })
            }
            Message::Restart(focus, placement) => {
                self.restart_for_update(focus.as_ref(), placement);
                Task::none()
            }
            Message::AutoInstallUpdates(on) => {
                self.settings.auto_install_updates = on;
                self.save_settings();
                match &self.updater {
                    UpdateState::Available {
                        release,
                        error: None,
                    } if on => {
                        let release = release.clone();
                        self.install_update(release)
                    }
                    _ => Task::none(),
                }
            }
            Message::AutoRestartUpdates(on) => {
                self.settings.auto_restart_updates = on;
                self.save_settings();
                Task::none()
            }
            Message::ResumeFocus(shown) => {
                if shown {
                    self.apply_pending_focus()
                } else {
                    Task::none()
                }
            }

            Message::OpenNewTask { idea } => {
                if self.selected_project().is_none() || self.project_repos().is_empty() {
                    return Task::none();
                }
                if self.page != Page::Main {
                    self.page = Page::Main;
                    self.reset_drafts();
                }
                if matches!(self.expanded, Some(Expanded::Task(_))) {
                    self.reset_drafts();
                }
                self.expanded = Some(Expanded::New { idea });
                self.collapsed.remove(if idea {
                    &Group::Ideas
                } else {
                    &Group::Proposals
                });
                Task::batch([operation::focus("proposal-title"), reveal("new-task")])
            }
            Message::OpenSettings => {
                if self.page != Page::Settings {
                    self.reset_drafts();
                }
                self.expanded = None;
                self.page = Page::Settings;
                self.sync_project_form();
                Task::none()
            }
            Message::OpenLicenses => {
                self.page = Page::Licenses;
                self.license_open = None;
                operation::snap_to(pages::BODY_ID, operation::RelativeOffset::START)
            }
            Message::ToggleLicense(i) => {
                self.license_open = (self.license_open != Some(i)).then_some(i);
                Task::none()
            }
            Message::OpenTask(number) => {
                let Some((room, project, group)) = self.selected_state().and_then(|s| {
                    let task = s.tasks.iter().find(|t| t.number == number)?;
                    Some((s.id, task.project, Group::of(task)))
                }) else {
                    return Task::none();
                };
                if self.chosen_projects.insert(room, project) != Some(project) {
                    self.sync_project_form();
                }
                self.page = Page::Main;
                if self.expanded != Some(Expanded::Task(number)) {
                    self.reset_drafts();
                }
                self.expanded = Some(Expanded::Task(number));
                self.collapsed.remove(&group);
                reveal("open-task")
            }
            Message::ToggleTask(number) => {
                if self.expanded == Some(Expanded::Task(number)) {
                    self.expanded = None;
                    self.reset_drafts();
                    return Task::none();
                }
                self.reset_drafts();
                self.expanded = Some(Expanded::Task(number));
                reveal("open-task")
            }
            Message::Back => {
                if self.expanded.take().is_some() {
                    self.reset_drafts();
                } else if self.page == Page::Licenses {
                    // Back to the About block, the last one on Settings.
                    self.page = Page::Settings;
                    return operation::snap_to_end(pages::BODY_ID);
                } else if self.page != Page::Main {
                    self.page = Page::Main;
                    self.reset_drafts();
                }
                Task::none()
            }
            Message::ToggleGroup(group) => {
                if !self.collapsed.remove(&group) {
                    self.collapsed.insert(group);
                }
                Task::none()
            }
            Message::ToggleSessions => {
                self.sessions_open = !self.sessions_open;
                Task::none()
            }
            Message::ToggleMembers => {
                self.members_open = !self.members_open;
                Task::none()
            }

            Message::SelectRoom(id) => {
                self.select(Some(id));
                Task::none()
            }
            Message::ServerUrl(s) => {
                self.forms.server_url = s;
                Task::none()
            }
            Message::RoomName(s) => {
                self.forms.room_name = s;
                Task::none()
            }
            Message::RoomKeyInput(s) => {
                self.forms.room_key = s;
                Task::none()
            }
            Message::SubmitCreate => self.submit_create(),
            Message::Created(result) => self.on_created(result),
            Message::SubmitJoin => self.submit_join(),
            Message::Joined(result) => self.on_joined(result),
            Message::CopyKey => match self.selected_key() {
                Some(key) => {
                    let key = key.to_string();
                    self.notify("Room key copied");
                    iced::clipboard::write(key)
                }
                None => Task::none(),
            },
            Message::LeaveRoom => match self.selected {
                Some(room) => {
                    let client = self.client.clone();
                    call(async move { client.leave_room(room).await }, Message::Left)
                }
                None => Task::none(),
            },
            Message::Left(result) => {
                match result {
                    Ok(()) => self.sync_rooms(),
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
            Message::RemoveRoom => {
                if let Some(room) = self.selected {
                    self.client.forget_room(room);
                    self.sync_rooms();
                    self.notify("Room removed from this app");
                }
                Task::none()
            }
            Message::NameInput(s) => {
                self.forms.name = s;
                Task::none()
            }
            Message::SubmitName => self.submit_name(),
            Message::RepoInput(s) => {
                self.forms.repo = s;
                Task::none()
            }
            Message::AddRepo => match self.link_repo(expand_home(self.forms.repo.trim())) {
                Some(task) => {
                    self.forms.repo.clear();
                    task
                }
                None => Task::none(),
            },
            Message::LinkRepo(path) => self.link_repo(path).unwrap_or_else(Task::none),
            Message::ChooseFolder(for_) => {
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
                let typed = expand_home(typed);
                let start = if typed.is_dir() {
                    Some(typed)
                } else {
                    match for_ {
                        FolderFor::Repo => self.searched_dirs().first().map(|p| p.to_path_buf()),
                        FolderFor::ProjectsDir => {
                            directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
                        }
                    }
                };
                Task::perform(choose_folder(title, start), move |path| {
                    Message::FolderChosen(for_, path)
                })
            }
            Message::FolderChosen(for_, path) => {
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
            Message::RemoveRepo(index) => {
                let (Some(room), Some(project)) = (self.selected, self.selected_project()) else {
                    return Task::none();
                };
                let key = settings::repo_key(&room, project.id);
                if let Some(repos) = self.settings.repos.get_mut(&key) {
                    if index < repos.len() {
                        repos.remove(index);
                    }
                    if repos.is_empty() {
                        self.settings.repos.remove(&key);
                    }
                }
                self.save_settings();
                self.publish()
            }
            Message::InstallPlugin(index) => self.install_plugin(index),
            Message::RemovePlugin(index) => self.remove_plugin(index),
            Message::UpdatePlugin(index) => {
                if let Some(repo) = self.project_repos().get(index).cloned() {
                    match self.update_plugin(&repo) {
                        Ok(()) => self.notify("omp plugin updated"),
                        Err(e) => self.banner = Some(update_failed(&repo, &e)),
                    }
                }
                Task::none()
            }
            Message::UpdateAllPlugins => {
                self.update_all_plugins();
                Task::none()
            }
            Message::AutoUpdatePlugins(on) => {
                self.settings.auto_update_plugins = on;
                self.save_settings();
                if on {
                    self.auto_failed.clear();
                    self.check_plugins();
                }
                Task::none()
            }

            Message::SelectProject(id) => {
                if let Some(room) = self.selected {
                    self.chosen_projects.insert(room, id);
                }
                self.expanded = None;
                self.reset_drafts();
                self.sync_project_form();
                // A repo cloned a moment ago shows as soon as its project is opened.
                self.read_project_dirs();
                Task::none()
            }
            Message::OpenNewProject => {
                self.open_settings();
                operation::focus("new-project-name")
            }
            Message::OpenProjectsDirs => {
                self.open_settings();
                operation::focus("projects-dir-input")
            }
            Message::NewProjectName(s) => {
                self.forms.new_project = s;
                Task::none()
            }
            Message::CreateProject => {
                let name = logic::truncate(self.forms.new_project.trim(), caps::PROJECT_NAME);
                let Some(room) = self.selected.filter(|_| !name.is_empty()) else {
                    return Task::none();
                };
                call(
                    self.request(room, Request::CreateProject { room, name }),
                    move |result| Message::ProjectCreated(room, result),
                )
            }
            Message::ProjectCreated(room, result) => {
                match result {
                    Ok(Response::Project(project)) => {
                        self.forms.new_project.clear();
                        self.chosen_projects.insert(room, project.id);
                        if self.selected == Some(room) {
                            self.page = Page::Main;
                            self.expanded = None;
                            self.reset_drafts();
                        }
                        self.sync_project_form();
                        self.read_project_dirs();
                        self.notify("Project created");
                    }
                    Ok(other) => self.banner = Some(format!("Unexpected reply: {other:?}")),
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
            Message::ProjectNameInput(s) => {
                self.forms.project_name = s;
                Task::none()
            }
            Message::RenameProject => {
                let name = logic::truncate(self.forms.project_name.trim(), caps::PROJECT_NAME);
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
            Message::ProjectRenamed(result) => {
                match result {
                    Ok(Response::Project(project)) => {
                        // The rename event may come after the reply: show the new name now.
                        self.forms.project_name = project.name;
                        self.notify("Project renamed");
                    }
                    Ok(other) => self.banner = Some(format!("Unexpected reply: {other:?}")),
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
            Message::MoveTask(project) => {
                let Some((state, task)) = self.open() else {
                    return Task::none();
                };
                if task.project == project {
                    return Task::none();
                }
                let room = state.id;
                let req = Request::MoveTask {
                    room,
                    task: task.number,
                    project,
                };
                call(self.request(room, req), Message::TaskMoved)
            }
            Message::TaskMoved(result) => {
                match result {
                    Ok(Response::Task(task)) => {
                        let target = self
                            .selected_state()
                            .and_then(|s| s.projects.iter().find(|p| p.id == task.project))
                            .map_or_else(|| format!("#{}", task.project), |p| p.name.clone());
                        if self.expanded == Some(Expanded::Task(task.number)) {
                            self.expanded = None;
                            self.reset_drafts();
                        }
                        self.notify(&format!("#{} moved to {target}", task.number));
                    }
                    Ok(other) => self.banner = Some(format!("Unexpected reply: {other:?}")),
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
            Message::ProjectsDirInput(s) => {
                self.forms.new_projects_dir = s;
                Task::none()
            }
            Message::AddProjectsDir => {
                let input = self.forms.new_projects_dir.trim();
                if input.is_empty() {
                    return Task::none();
                }
                if self.add_projects_dir(expand_home(input)) {
                    self.forms.new_projects_dir.clear();
                }
                Task::none()
            }
            Message::RemoveProjectsDir(index) => {
                // An emptied list stays `Some`, so the defaults are not entered again.
                let Some(dirs) = self
                    .settings
                    .projects_dirs
                    .as_mut()
                    .filter(|dirs| index < dirs.len())
                else {
                    return Task::none();
                };
                dirs.remove(index);
                self.save_settings();
                self.read_project_dirs();
                self.notify("Projects folder removed");
                Task::none()
            }

            Message::ProposalTitle(s) => {
                self.forms.proposal_title = s;
                Task::none()
            }
            Message::ProposalBody(action) => {
                self.forms.proposal_body.perform(action);
                Task::none()
            }
            Message::Propose => self.propose(),
            Message::Proposed(result) => {
                match result {
                    Ok(response) => {
                        self.forms.proposal_title.clear();
                        self.forms.proposal_body = text_editor::Content::new();
                        if matches!(self.expanded, Some(Expanded::New { .. })) {
                            self.expanded = None;
                        }
                        let idea = matches!(&response, Response::Task(t) if t.stage == Stage::Idea);
                        self.notify(if idea { "Idea added" } else { "Task proposed" });
                    }
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
            Message::Pick(number) => self.pick(number),
            Message::Picked(result) => {
                self.picking = None;
                match result {
                    Ok((room, task)) => match &task.claim {
                        Some(claim) => {
                            let prompt = logic::pick_prompt(&room, &task, &claim.token);
                            self.notify(&format!("#{} is yours, prompt copied", task.number));
                            return iced::clipboard::write(prompt);
                        }
                        None => self.banner = Some("The claim came back empty".to_owned()),
                    },
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }

            Message::EditTask => {
                if let Some((_, task)) = self.open() {
                    self.task_draft = Some(TaskDraft {
                        title: task.title.clone(),
                        body: text_editor::Content::with_text(&task.body),
                    });
                }
                operation::focus("task-title")
            }
            Message::CancelEditTask => {
                self.task_draft = None;
                Task::none()
            }
            Message::EditTitle(s) => {
                if let Some(draft) = &mut self.task_draft {
                    draft.title = s;
                }
                Task::none()
            }
            Message::EditBody(action) => {
                if let Some(draft) = &mut self.task_draft {
                    draft.body.perform(action);
                }
                Task::none()
            }
            Message::SaveTask => self.save_task(),
            Message::ProposeIdea => self.propose_idea(),
            Message::TaskSaved(result) => {
                match result {
                    Ok(_) => self.task_draft = None,
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
            Message::EditPlan => {
                if let Some((_, task)) = self.open() {
                    self.plan_draft = Some(text_editor::Content::with_text(&task.plan));
                }
                Task::none()
            }
            Message::CancelEditPlan => {
                self.plan_draft = None;
                Task::none()
            }
            Message::PlanAction(action) => {
                if let Some(draft) = &mut self.plan_draft {
                    draft.perform(action);
                }
                Task::none()
            }
            Message::SavePlan => self.save_plan(),
            Message::PlanSaved(result) => {
                match result {
                    Ok(_) => self.plan_draft = None,
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
            Message::SetPriority(priority) => match self.open_ids() {
                Some((room, task)) => fire(self.request(
                    room,
                    Request::SetPriority {
                        room,
                        task,
                        priority,
                    },
                )),
                None => Task::none(),
            },
            Message::Decide(number, decision) => match self.task_ids(number) {
                Some((room, task)) => fire(self.request(
                    room,
                    Request::Decide {
                        room,
                        task,
                        decision,
                    },
                )),
                None => Task::none(),
            },
            Message::Release(number) => match self.task_ids(number) {
                Some((room, task)) => fire(self.request(
                    room,
                    Request::Release {
                        room,
                        task,
                        note: String::new(),
                    },
                )),
                None => Task::none(),
            },
            Message::NoteInput(s) => {
                self.forms.note = s;
                Task::none()
            }
            Message::AddNote => self.add_note(),
            Message::NoteAdded(result) => {
                match result {
                    Ok(_) => self.forms.note.clear(),
                    Err(e) => self.banner = Some(e),
                }
                Task::none()
            }
        }
    }

    // ---- lookups ------------------------------------------------------------------------

    fn selected_state(&self) -> Option<&RoomState> {
        let selected = self.selected?;
        self.rooms
            .iter()
            .find(|(key, _)| RoomId::of(&key.token) == selected)
            .and_then(|(_, state)| state.as_ref())
    }

    fn selected_key(&self) -> Option<&RoomKey> {
        let selected = self.selected?;
        self.rooms
            .iter()
            .map(|(key, _)| key)
            .find(|key| RoomId::of(&key.token) == selected)
    }

    /// Whether the selected room's server answers; `None` before it said either.
    fn selected_server_online(&self) -> Option<bool> {
        let key = self.selected_key()?;
        self.servers.get(&key.server).map(|s| s.online)
    }

    /// The status-bar tooltip of the selected room's server.
    fn selected_server_tip(&self) -> String {
        let Some(key) = self.selected_key() else {
            return "No room selected".to_owned();
        };
        let server = format!("{}:{}", key.server.host, key.server.port);
        match self.servers.get(&key.server) {
            Some(ServerStatus { online: true, .. }) => format!("Server {server} online"),
            Some(ServerStatus { error: Some(e), .. }) => format!("Server {server} offline: {e}"),
            Some(_) => format!("Server {server} offline"),
            None => format!("Connecting to {server}"),
        }
    }

    /// The rooms for the switcher, in list order.
    fn room_choices(&self) -> Vec<RoomChoice> {
        self.rooms
            .iter()
            .map(|(key, state)| {
                let id = RoomId::of(&key.token);
                RoomChoice {
                    id,
                    name: state
                        .as_ref()
                        .map_or_else(|| format!("Room {}", &id.to_hex()[..8]), |s| s.name.clone()),
                }
            })
            .collect()
    }

    /// The project shown for the selected room: the one last picked, else the first one this
    /// member has linked a repo to, else the first one; `None` while the room has none.
    fn selected_project(&self) -> Option<&Project> {
        let state = self.selected_state()?;
        let chosen = self.chosen_projects.get(&state.id);
        chosen
            .and_then(|id| state.projects.iter().find(|p| p.id == *id))
            .or_else(|| {
                state
                    .projects
                    .iter()
                    .find(|p| !self.settings.project_repos(&state.id, p.id).is_empty())
            })
            .or_else(|| state.projects.first())
    }

    /// The repos this member has linked to the selected project.
    fn project_repos(&self) -> &[PathBuf] {
        match (self.selected, self.selected_project()) {
            (Some(room), Some(project)) => self.settings.project_repos(&room, project.id),
            _ => &[],
        }
    }

    /// Puts the selected project's name into the rename input.
    fn sync_project_form(&mut self) {
        self.forms.project_name = self
            .selected_project()
            .map(|p| p.name.clone())
            .unwrap_or_default();
    }

    /// Opens Settings, keeping drafts when it is open already.
    fn open_settings(&mut self) {
        if self.page != Page::Settings {
            self.reset_drafts();
        }
        self.expanded = None;
        self.page = Page::Settings;
        self.sync_project_form();
    }

    /// Reads the subfolders of the searched folders.
    fn read_project_dirs(&mut self) {
        let mut dirs: Vec<PathBuf> = self
            .searched_dirs()
            .into_iter()
            .flat_map(settings::subdirs)
            .collect();
        dirs.sort();
        dirs.dedup();
        self.project_dirs = dirs;
    }

    /// Enters the default projects folders of every running distro seen for the first time.
    /// A distro whose homes could not be read yet (it may still be starting) is left for a
    /// later round.
    fn enter_wsl_projects_dirs(&mut self) {
        let mut changed = false;
        for distro in self.wsl.iter().filter(|d| !d.homes.is_empty()) {
            changed |= self
                .settings
                .enter_wsl_distro(&distro.name, &distro.projects_dirs);
        }
        if changed {
            self.save_settings();
        }
    }

    /// The directories omp sessions are read from: the configured one, then the one under
    /// every home of every running WSL distro.
    fn sessions_dirs(&self) -> Vec<sessions::SessionsDir> {
        let configured = sessions::SessionsDir {
            path: self.settings.omp_sessions_dir.clone(),
            distro: None,
        };
        let in_wsl = self.wsl.iter().flat_map(|d| {
            d.homes.iter().map(|home| sessions::SessionsDir {
                path: home.join(".omp").join("agent").join("sessions"),
                distro: Some(d.name.clone()),
            })
        });
        std::iter::once(configured).chain(in_wsl).collect()
    }

    /// The folders whose subfolders are suggested as repos: the projects folders, without
    /// those in a WSL distro that is not running.
    fn searched_dirs(&self) -> Vec<&Path> {
        self.settings
            .projects_dirs
            .iter()
            .flatten()
            .filter(|dir| !self.in_stopped_distro(dir))
            .map(PathBuf::as_path)
            .collect()
    }

    /// Whether `path` lies in a WSL distro that is not running: such a path is not read, as
    /// that would start the distro.
    fn in_stopped_distro(&self, path: &Path) -> bool {
        wsl::distro_of(path)
            .is_some_and(|name| !self.wsl.iter().any(|d| d.name.eq_ignore_ascii_case(name)))
    }

    /// The selected room and the open task.
    fn open(&self) -> Option<(&RoomState, &RoomTask)> {
        let Some(Expanded::Task(number)) = self.expanded else {
            return None;
        };
        let state = self.selected_state()?;
        let task = state.tasks.iter().find(|t| t.number == number)?;
        Some((state, task))
    }

    fn open_ids(&self) -> Option<(RoomId, u64)> {
        self.open().map(|(state, task)| (state.id, task.number))
    }

    /// The selected room's id with task `number`, for the buttons on a slab.
    fn task_ids(&self, number: u64) -> Option<(RoomId, u64)> {
        self.selected_state().map(|s| (s.id, number))
    }

    // ---- calls --------------------------------------------------------------------------

    fn request(
        &self,
        room: RoomId,
        req: Request,
    ) -> impl Future<Output = Result<Response, ClientError>> + Send + 'static + use<> {
        let client = self.client.clone();
        async move { client.request(room, req).await }
    }

    fn submit_create(&mut self) -> Task<Message> {
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
        let name = logic::truncate(name, caps::ROOM_NAME);
        let client = self.client.clone();
        call(
            async move { client.create_room(server, name).await },
            Message::Created,
        )
    }

    fn on_created(&mut self, result: Result<RoomKey, String>) -> Task<Message> {
        match result {
            Ok(key) => {
                self.settings.last_server = Some(key.server.to_string());
                self.forms.room_name.clear();
                self.sync_rooms();
                self.select(Some(RoomId::of(&key.token)));
                self.save_settings();
                self.publish()
            }
            Err(e) => {
                self.banner = Some(e);
                Task::none()
            }
        }
    }

    fn submit_join(&mut self) -> Task<Message> {
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

    fn on_joined(&mut self, result: Result<RoomId, String>) -> Task<Message> {
        match result {
            Ok(room) => {
                self.forms.room_key.clear();
                self.sync_rooms();
                self.select(Some(room));
                self.save_settings();
                self.publish()
            }
            Err(e) => {
                self.banner = Some(e);
                Task::none()
            }
        }
    }

    fn submit_name(&mut self) -> Task<Message> {
        let name = logic::truncate(self.forms.name.trim(), caps::MEMBER_NAME);
        if name.is_empty() {
            self.banner = Some("Name is empty".to_owned());
            return Task::none();
        }
        self.forms.name.clone_from(&name);
        self.settings.name.clone_from(&name);
        self.save_settings();
        self.notify("Name saved");
        let client = self.client.clone();
        Task::perform(async move { client.set_name(name).await }, |()| {
            Message::Noop
        })
    }

    /// Adds `path` to the projects folders; `false` when it is not a folder on this machine
    /// (with a banner). A folder already listed counts as added.
    fn add_projects_dir(&mut self, path: PathBuf) -> bool {
        let path = wsl::canonical(path);
        if !path.is_absolute() || !path.is_dir() {
            self.banner = Some(format!("Not a folder on this machine: {}", path.display()));
            return false;
        }
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

    /// Links `path` to the selected project; `None` when nothing was linked.
    fn link_repo(&mut self, path: PathBuf) -> Option<Task<Message>> {
        let path = wsl::canonical(path);
        let room = self.selected?;
        let project = self.selected_project()?.id;
        if !path.is_absolute() || !path.is_dir() {
            self.banner = Some(format!("Not a folder on this machine: {}", path.display()));
            return None;
        }
        let repos = self
            .settings
            .repos
            .entry(settings::repo_key(&room, project))
            .or_default();
        if !repos.contains(&path) {
            repos.push(path);
        }
        self.save_settings();
        Some(self.publish())
    }

    fn install_plugin(&mut self, index: usize) -> Task<Message> {
        let Some(repo) = self.project_repos().get(index).cloned() else {
            return Task::none();
        };
        match self.update_plugin(&repo) {
            Ok(()) => {
                self.notify("omp plugin installed");
            }
            Err(e) => {
                self.banner = Some(format!(
                    "Cannot install the omp plugin in {}: {e}",
                    repo.display()
                ));
            }
        }
        Task::none()
    }

    fn remove_plugin(&mut self, index: usize) -> Task<Message> {
        let Some(repo) = self.project_repos().get(index).cloned() else {
            return Task::none();
        };
        match omp_plugin::uninstall(&repo) {
            Ok(()) => {
                self.plugins.remove(&repo);
                self.auto_failed.remove(&repo);
                self.notify("omp plugin removed");
            }
            Err(e) => {
                self.banner = Some(format!(
                    "Cannot remove the omp plugin from {}: {e}",
                    repo.display()
                ));
            }
        }
        Task::none()
    }

    fn plugin_config(
        &self,
        repo: &Path,
        projects: Vec<omp_plugin::PluginProject>,
    ) -> omp_plugin::PluginConfig {
        omp_plugin::PluginConfig {
            data_dir: wsl::path_for(repo, &self.data_dir),
            mcp_url: mcp::url(self.settings.mcp_port),
            projects,
            other_repos: omp_plugin::other_repos(&self.settings.repos, repo),
        }
    }

    /// Writes the plugin into `repo` for the projects it is linked to and marks it current.
    fn update_plugin(&mut self, repo: &Path) -> std::io::Result<()> {
        let projects = omp_plugin::projects_of(&self.settings.repos, repo);
        omp_plugin::install(repo, &self.plugin_config(repo, projects))?;
        self.plugins.insert(repo.to_owned(), PluginState::Current);
        self.auto_failed.remove(repo);
        Ok(())
    }

    /// Updates every out-of-date plugin, in all rooms; the first failure goes to the banner.
    fn update_all_plugins(&mut self) {
        let outdated: Vec<PathBuf> = self
            .plugins
            .iter()
            .filter(|(_, state)| **state == PluginState::Outdated)
            .map(|(repo, _)| repo.clone())
            .collect();
        let mut updated = 0;
        let mut failure = None;
        for repo in outdated {
            match self.update_plugin(&repo) {
                Ok(()) => updated += 1,
                Err(e) => {
                    failure.get_or_insert_with(|| update_failed(&repo, &e));
                }
            }
        }
        if let Some(failure) = failure {
            self.banner = Some(failure);
        }
        match updated {
            0 => {}
            1 => self.notify("omp plugin updated"),
            n => self.notify(&format!("omp plugin updated in {n} repos")),
        }
    }

    /// Compares the plugin in every shared repo with this app's and, with auto-update on,
    /// rewrites the out-of-date ones whose automatic update has not failed before.
    fn check_plugins(&mut self) {
        self.plugins_checked_ms = now_ms();
        let shared: BTreeSet<PathBuf> = self.settings.repos.values().flatten().cloned().collect();
        for repo in &shared {
            // A repo in a stopped distro is not read: that would start the distro.
            if self.in_stopped_distro(repo) {
                continue;
            }
            let projects = omp_plugin::projects_of(&self.settings.repos, repo);
            match omp_plugin::state(repo, &self.plugin_config(repo, projects)) {
                Some(state) => {
                    self.plugins.insert(repo.clone(), state);
                }
                None => {
                    self.plugins.remove(repo);
                }
            }
        }
        self.plugins.retain(|repo, _| shared.contains(repo));
        let plugins = &self.plugins;
        self.auto_failed
            .retain(|repo| plugins.get(repo) == Some(&PluginState::Outdated));
        if !self.settings.auto_update_plugins {
            return;
        }
        let outdated: Vec<PathBuf> = self
            .plugins
            .iter()
            .filter(|(repo, state)| {
                **state == PluginState::Outdated && !self.auto_failed.contains(*repo)
            })
            .map(|(repo, _)| repo.clone())
            .collect();
        let mut updated = false;
        for repo in outdated {
            match self.update_plugin(&repo) {
                Ok(()) => updated = true,
                Err(e) => {
                    self.auto_failed.insert(repo.clone());
                    self.banner = Some(update_failed(&repo, &e));
                }
            }
        }
        if updated {
            self.notify("omp plugin updated");
        }
    }

    /// Rewrites the plugin in every repo that has it, and removes it from a repo no room
    /// is linked to any more. Runs regardless of auto-update: `schwitzer.json` lists the
    /// projects.
    fn sync_plugins(&mut self) {
        for repo in self.plugins.keys().cloned().collect::<Vec<_>>() {
            let result = if omp_plugin::projects_of(&self.settings.repos, &repo).is_empty() {
                omp_plugin::uninstall(&repo).map(|()| {
                    self.plugins.remove(&repo);
                    self.auto_failed.remove(&repo);
                })
            } else {
                self.update_plugin(&repo)
            };
            if let Err(e) = result {
                self.banner = Some(format!("omp plugin in {}: {e}", repo.display()));
            }
        }
    }

    fn propose(&mut self) -> Task<Message> {
        let Some(Expanded::New { idea }) = self.expanded else {
            return Task::none();
        };
        let (Some(room), Some(project), title) = (
            self.selected,
            self.selected_project().map(|p| p.id),
            self.forms.proposal_title.trim(),
        ) else {
            return Task::none();
        };
        if title.is_empty() {
            return Task::none();
        }
        let (title, body) = (title.to_owned(), self.forms.proposal_body.text());
        let req = if idea {
            Request::AddIdea {
                room,
                project,
                title,
                body,
            }
        } else {
            Request::Propose {
                room,
                project,
                title,
                body,
            }
        };
        call(self.request(room, req), Message::Proposed)
    }

    /// Approves the plan when it still waits for that, claims the build for this member and
    /// hands back the room name and the claimed task for the prompt.
    fn pick(&mut self, number: u64) -> Task<Message> {
        if self.picking.is_some() {
            return Task::none();
        }
        let Some(state) = self.selected_state() else {
            return Task::none();
        };
        let Some(task) = state.tasks.iter().find(|t| t.number == number) else {
            return Task::none();
        };
        if !logic::pickable(task) {
            return Task::none();
        }
        let (room, name, approve) = (state.id, state.name.clone(), task.stage == Stage::Planned);
        self.picking = Some(number);
        let client = self.client.clone();
        call(
            async move {
                if approve {
                    let decision = Decision::Approve;
                    client
                        .request(
                            room,
                            Request::Decide {
                                room,
                                task: number,
                                decision,
                            },
                        )
                        .await?;
                }
                let claim = Request::Claim {
                    room,
                    work: Work::Build,
                    project: None,
                    task: Some(number),
                    agent: PICK_AGENT.to_owned(),
                };
                match client.request(room, claim).await? {
                    Response::Claimed(Some(task)) => Ok((name, task)),
                    other => Err(ClientError::Proto(format!("unexpected reply: {other:?}"))),
                }
            },
            Message::Picked,
        )
    }

    fn save_task(&mut self) -> Task<Message> {
        let (Some((state, task)), Some(draft)) = (self.open(), &self.task_draft) else {
            return Task::none();
        };
        if draft.title.trim().is_empty() {
            return Task::none();
        }
        let room = state.id;
        let req = Request::EditTask {
            room,
            task: task.number,
            title: draft.title.clone(),
            body: draft.body.text(),
            plan: task.plan.clone(),
        };
        call(self.request(room, req), Message::TaskSaved)
    }

    /// Writes the open idea up into a proposal with the draft's title and body.
    fn propose_idea(&self) -> Task<Message> {
        let (Some((state, task)), Some(draft)) = (self.open(), &self.task_draft) else {
            return Task::none();
        };
        if task.stage != Stage::Idea || draft.title.trim().is_empty() {
            return Task::none();
        }
        let room = state.id;
        let req = Request::ProposeIdea {
            room,
            task: task.number,
            title: draft.title.clone(),
            body: draft.body.text(),
        };
        call(self.request(room, req), Message::TaskSaved)
    }

    fn save_plan(&self) -> Task<Message> {
        let (Some((state, task)), Some(draft)) = (self.open(), &self.plan_draft) else {
            return Task::none();
        };
        let room = state.id;
        let req = Request::EditTask {
            room,
            task: task.number,
            title: task.title.clone(),
            body: task.body.clone(),
            plan: draft.text(),
        };
        call(self.request(room, req), Message::PlanSaved)
    }

    fn add_note(&mut self) -> Task<Message> {
        let Some((room, task)) = self.open_ids() else {
            return Task::none();
        };
        let text = self.forms.note.trim();
        if text.is_empty() {
            return Task::none();
        }
        let req = Request::AddNote {
            room,
            task,
            text: text.to_owned(),
            agent: None,
        };
        call(self.request(room, req), Message::NoteAdded)
    }

    // ---- client events and rooms ----------------------------------------------------------

    fn on_client(&mut self, event: ClientEvent) -> Task<Message> {
        match event {
            ClientEvent::Status {
                server,
                online,
                error,
            } => {
                if online {
                    if let Some(e) = &error {
                        self.banner = Some(format!("{}:{}: {e}", server.host, server.port));
                    }
                    // A server that came back lost the sessions of this member.
                    let stale: Vec<RoomId> = self
                        .rooms
                        .iter()
                        .filter(|(key, _)| key.server == server)
                        .map(|(key, _)| RoomId::of(&key.token))
                        .collect();
                    self.published.retain(|room, _| !stale.contains(room));
                    self.servers.insert(
                        server,
                        ServerStatus {
                            online,
                            error: None,
                        },
                    );
                    self.publish()
                } else {
                    // An offline server's announcement no longer counts; the next periodic check
                    // picks up the change, so a flapping connection asks GitHub no more often.
                    self.releases.remove(&server);
                    self.servers.insert(server, ServerStatus { online, error });
                    Task::none()
                }
            }
            ClientEvent::Room(state) => {
                let id = state.id;
                match self
                    .rooms
                    .iter_mut()
                    .find(|(key, _)| RoomId::of(&key.token) == id)
                {
                    Some(entry) => entry.1 = Some(*state),
                    None => self.sync_rooms(),
                }
                if self.selected.is_none() {
                    self.select(Some(id));
                }
                self.publish()
            }
            ClientEvent::RoomRemoved(_) => {
                // A removed room may take the announcement the update followed with it.
                let before = self.release_target();
                self.sync_rooms();
                self.follow_release(before)
            }
            ClientEvent::Release { server, version } => {
                let Ok(version) = version.parse::<semver::Version>() else {
                    tracing::warn!(
                        "{}:{} announced an unreadable version {version:?}",
                        server.host,
                        server.port
                    );
                    return Task::none();
                };
                let before = self.release_target();
                self.releases.insert(server, version);
                self.follow_release(before)
            }
        }
    }

    /// Takes the rooms from the client, keeps the selection valid and saves the room list.
    fn sync_rooms(&mut self) {
        self.rooms = self.client.rooms();
        self.published.retain(|room, _| {
            self.rooms
                .iter()
                .any(|(k, _)| RoomId::of(&k.token) == *room)
        });
        let selected_gone = self.selected.is_some_and(|selected| {
            !self
                .rooms
                .iter()
                .any(|(key, _)| RoomId::of(&key.token) == selected)
        });
        if selected_gone || self.selected.is_none() {
            let first = self.rooms.first().map(|(key, _)| RoomId::of(&key.token));
            self.select(first);
        }
        self.settings.rooms = self.rooms.iter().map(|(key, _)| key.to_string()).collect();
        let joined: HashSet<String> = self
            .rooms
            .iter()
            .map(|(key, _)| RoomId::of(&key.token).to_hex())
            .collect();
        self.settings.repos.retain(|key, _| {
            settings::parse_repo_key(key).is_some_and(|(room, _)| joined.contains(room))
        });
        self.save_settings();
    }

    fn select(&mut self, room: Option<RoomId>) {
        if self.selected != room {
            self.selected = room;
            self.expanded = None;
            self.reset_drafts();
            self.sync_project_form();
        }
    }

    fn reset_drafts(&mut self) {
        self.task_draft = None;
        self.plan_draft = None;
        self.forms.note.clear();
    }

    // ---- self-update ----------------------------------------------------------------------

    /// The release to update to: the highest one the online servers of the kept rooms announced;
    /// `None` makes the check ask GitHub for its latest release.
    fn release_target(&self) -> Option<semver::Version> {
        self.releases
            .iter()
            .filter(|(server, _)| self.rooms.iter().any(|(key, _)| key.server == **server))
            .map(|(_, version)| version)
            .max()
            .cloned()
    }

    /// Checks again when the target moved.
    fn follow_release(&mut self, before: Option<semver::Version>) -> Task<Message> {
        if update::ENABLED && self.release_target() != before {
            self.check_update()
        } else {
            Task::none()
        }
    }

    /// Looks up the release the servers announced, or GitHub's latest one when none did, unless a
    /// check or an install is under way, or an update waits for its restart.
    fn check_update(&mut self) -> Task<Message> {
        if !update::ENABLED
            || matches!(
                self.updater,
                UpdateState::Checking | UpdateState::Installing(_) | UpdateState::Installed(_)
            )
        {
            return Task::none();
        }
        let Some(program) = &self.program else {
            self.updater = UpdateState::Failed("Cannot find this program's file".into());
            return Task::none();
        };
        let package = program.package;
        let target = self.release_target();
        if target
            .as_ref()
            .is_some_and(|version| *version <= update::current())
        {
            self.updater = UpdateState::UpToDate;
            return Task::none();
        }
        self.updater = UpdateState::Checking;
        Task::perform(update::check(target, package), Message::UpdateChecked)
    }

    fn update_checked(&mut self, result: Result<Option<update::Release>, String>) -> Task<Message> {
        if self.updater != UpdateState::Checking {
            return Task::none();
        }
        match result {
            Ok(None) => self.updater = UpdateState::UpToDate,
            Ok(Some(release))
                if self
                    .release_target()
                    .is_some_and(|target| target != release.version) =>
            {
                // The announcements moved while GitHub answered: look up the new target.
                self.updater = UpdateState::Idle;
                return self.check_update();
            }
            Ok(Some(release)) => {
                // A version that failed to install waits for the Install button.
                let error = self
                    .update_failed
                    .as_ref()
                    .filter(|(version, _)| *version == release.version)
                    .map(|(_, e)| e.clone());
                if error.is_none() && self.settings.auto_install_updates {
                    return self.install_update(release);
                }
                self.updater = UpdateState::Available { release, error };
            }
            // No banner: an offline machine would get one every five minutes.
            Err(e) => {
                tracing::warn!("update check failed: {e}");
                self.updater = UpdateState::Failed(e);
            }
        }
        Task::none()
    }

    fn install_update(&mut self, release: update::Release) -> Task<Message> {
        let Some(program) = self.program.clone() else {
            return Task::none();
        };
        self.updater = UpdateState::Installing(release.clone());
        Task::perform(update::install(release, program), Message::UpdateInstalled)
    }

    fn update_installed(&mut self, result: Result<semver::Version, String>) -> Task<Message> {
        let UpdateState::Installing(release) = &self.updater else {
            return Task::none();
        };
        let release = release.clone();
        match result {
            Ok(version) => {
                self.update_failed = None;
                self.updater = UpdateState::Installed(version.clone());
                if self.settings.auto_restart_updates {
                    return self.handle(Message::RestartForUpdate);
                }
                self.notify(&format!("Version {version} installed, restart to use it"));
            }
            Err(e) => {
                self.update_failed = Some((release.version.clone(), e.clone()));
                self.updater = UpdateState::Available {
                    release,
                    error: Some(e),
                };
            }
        }
        Task::none()
    }

    /// Writes what the window shows for the new process, starts it and exits. The exit skips
    /// every destructor, so nothing of the old process removes what the new one already set up
    /// (the remote control socket).
    fn restart_for_update(
        &mut self,
        focus: Option<&iced::advanced::widget::Id>,
        placement: Option<Placement>,
    ) {
        let (UpdateState::Installed(_), Some(program)) = (&self.updater, &self.program) else {
            return;
        };
        let snapshot = self.snapshot(focus, placement);
        if let Err(e) = resume::write(&self.data_dir, &snapshot) {
            self.banner = Some(format!("Cannot restart: {e}"));
            return;
        }
        if let Err(e) = update::restart(&program.path) {
            resume::remove(&self.data_dir);
            self.banner = Some(format!("Cannot restart: {e}"));
            return;
        }
        self.client.shutdown();
        std::process::exit(0);
    }

    /// After a restart for an update: looks whether the element to focus or reveal shows yet;
    /// gives up after a while (a task page needs the room from the server first).
    fn pending_focus(&mut self) -> Task<Message> {
        let Some(pending) = &self.resume_focus else {
            return Task::none();
        };
        if self.now_ms > pending.until_ms {
            self.resume_focus = None;
            return Task::none();
        }
        let target = pending.focus.or(match self.expanded {
            Some(Expanded::Task(_)) => Some("open-task"),
            Some(Expanded::New { .. }) => Some("new-task"),
            None => None,
        });
        let Some(target) = target else {
            self.resume_focus = None;
            return Task::none();
        };
        selector::find(selector::id(target)).map(|found| Message::ResumeFocus(found.is_some()))
    }

    /// Focuses and reveals what the window showed before the restart, now that it shows.
    fn apply_pending_focus(&mut self) -> Task<Message> {
        let Some(pending) = self.resume_focus.take() else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        if let Some(id) = pending.focus {
            tasks.push(operation::focus(id));
            // iced cannot read a text input's cursor: those resume at the end.
            if !resume::EDITORS.contains(&id) {
                tasks.push(operation::move_cursor_to_end(id));
            }
        }
        match self.expanded {
            Some(Expanded::Task(_)) => tasks.push(reveal("open-task")),
            Some(Expanded::New { .. }) => tasks.push(reveal("new-task")),
            None => {}
        }
        Task::batch(tasks)
    }

    /// Opens a web link from task text in the default browser. Other links are refused:
    /// the text comes from other members and their agents. A headless instance opens none.
    fn open_link(&mut self, url: &str) {
        if !logic::is_web_link(url) {
            self.notify("Only http and https links open.");
            return;
        }
        #[cfg(feature = "remote")]
        if iced_impeccable::is_headless() {
            tracing::info!("headless: link not opened: {url}");
            self.notify("Link not opened (headless).");
            return;
        }
        if let Err(e) = open::that_detached(url) {
            self.notify(&format!("Cannot open link: {e}"));
        }
    }

    fn notify(&mut self, text: &str) {
        self.notice = Some(Notice {
            text: text.to_owned(),
            until_ms: now_ms() + NOTICE_MS,
        });
    }

    /// Writes `settings.toml` when something changed since it was last read or written.
    /// Changed repos also update the omp plugins.
    fn save_settings(&mut self) {
        if self.settings == self.saved {
            return;
        }
        let repos_changed = self.settings.repos != self.saved.repos;
        match self.settings.save(&self.data_dir) {
            Ok(()) => self.saved = self.settings.clone(),
            Err(e) => self.banner = Some(format!("Cannot save settings: {e}")),
        }
        if repos_changed {
            self.sync_plugins();
        }
    }

    /// Re-parses the open task's details, plan and notes where their text changed.
    fn refresh_markdown(&mut self) {
        let mut md = std::mem::take(&mut self.open_md);
        md.sync(self.open().map(|(_, task)| task));
        self.open_md = md;
    }

    // ---- publishing -----------------------------------------------------------------------

    /// Sends each room the local sessions running in the repos shared with it, when that list
    /// changed since the last send. The first list per room (after start or reconnect) is sent
    /// even when empty: the server may still hold sessions of an earlier run whose connection
    /// has not timed out yet.
    fn publish(&mut self) -> Task<Message> {
        let mut calls = Vec::new();
        for (key, state) in &self.rooms {
            let Some(state) = state else { continue };
            let room = RoomId::of(&key.token);
            let repos = self.settings.room_project_repos(&room);
            let sessions =
                logic::shared_sessions(&self.local_sessions, state, self.client.member(), &repos);
            if self.published.get(&room) == Some(&sessions) {
                continue;
            }
            self.published.insert(room, sessions.clone());
            let client = self.client.clone();
            calls.push(call(
                async move {
                    client
                        .request(room, Request::PublishSessions { room, sessions })
                        .await
                },
                move |result| Message::Published(room, result.map(|_| ())),
            ));
        }
        Task::batch(calls)
    }
}

/// The app's keyboard shortcuts, from key presses no widget took.
fn key_message(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    match key.as_ref() {
        Key::Named(Named::Escape) => Some(Message::Back),
        Key::Named(Named::Tab) if modifiers.shift() => Some(Message::FocusPrevious),
        Key::Named(Named::Tab) => Some(Message::FocusNext),
        Key::Character("n") if modifiers.command() => Some(Message::OpenNewTask { idea: false }),
        Key::Character(",") if modifiers.command() => Some(Message::OpenSettings),
        _ => None,
    }
}

/// Scrolls the task list so the element with widget id `id` shows: when its top is above the
/// list, or its first list-height is below it, its top goes to the top of the list. Runs after
/// the next layout, so it finds an element the same update opened.
fn reveal(id: &'static str) -> Task<Message> {
    selector::find(selector::id(list::LIST_ID)).then(move |list| {
        let Some(Target::Scrollable {
            bounds,
            content_bounds,
            translation,
            ..
        }) = list
        else {
            return Task::none();
        };
        selector::find(selector::id(id)).then(move |target| {
            let Some(target) = target else {
                return Task::none();
            };
            let el = target.bounds();
            let top = el.y - translation.y;
            let bottom = top + el.height.min(bounds.height);
            if top >= bounds.y && bottom <= bounds.y + bounds.height {
                return Task::none();
            }
            operation::scroll_to(
                list::LIST_ID,
                AbsoluteOffset {
                    x: 0.0,
                    y: (el.y - content_bounds.y - SP_2).max(0.0),
                },
            )
        })
    })
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

/// `~/x` as a path under the home directory; anything else as given.
fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => directories::BaseDirs::new()
            .map_or_else(|| PathBuf::from(path), |dirs| dirs.home_dir().join(rest)),
        None => Path::new(path).to_path_buf(),
    }
}

/// `path` with the home directory shown as `~`.
fn tilde(path: &Path) -> String {
    let rest = directories::BaseDirs::new().and_then(|dirs| {
        path.strip_prefix(dirs.home_dir())
            .ok()
            .map(Path::to_path_buf)
    });
    match rest {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `path` as the member knows it: a folder in a WSL distro as its Linux path with the
/// distro, `/home/felix/code (Ubuntu)`; anything else with the home directory as `~`.
fn folder_label(path: &Path) -> String {
    match wsl::to_linux(path) {
        Some((distro, linux)) => format!("{linux} ({distro})"),
        None => tilde(path),
    }
}

/// The banner text for a plugin update that failed.
fn update_failed(repo: &Path, e: &std::io::Error) -> String {
    format!("Cannot update the omp plugin in {}: {e}", repo.display())
}

/// A start that cannot continue: no data directory, no identity.
fn fatal(message: &str) -> ! {
    eprintln!("schwitzer: {message}");
    std::process::exit(1)
}
