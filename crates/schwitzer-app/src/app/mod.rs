//! The application: state, messages and `update`. The views live in the submodules.

mod chrome;
mod gpu;
mod icons;
mod licenses_view;
mod list;
mod live;
pub mod logic;
mod pages;
mod plugins;
mod resume;
mod rooms;
mod settings_view;
mod sticky;
mod task_view;
mod updates;
mod widgets;

pub use self::resume::Resumed;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::operation::AbsoluteOffset;
use iced::widget::selector::{self, Target};
use iced::widget::{column, container, markdown, operation, stack, text_editor};
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
use self::rooms::Rooms;
use crate::bridge::{self, ClientKey, call, fire};
use crate::mcp::{self, McpStatus};
use crate::omp_plugin::PluginState;
use crate::sessions::{self, LocalSession};
use crate::settings::{self, Settings};
use crate::system_notify;
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
    /// Opens a page of Settings, or Settings itself, at its top.
    OpenPage(Page),
    /// Selects room `id` and opens its settings page.
    OpenRoom(RoomId),
    /// Shows project `id` of the selected room and opens its settings page.
    OpenProject(u64),
    /// Asks in place to confirm a danger action before it runs.
    Ask(Confirm),
    /// Drops the pending confirmation.
    CancelConfirm,
    /// Opens the About page with the licenses from Settings.
    OpenLicenses,
    /// Unfolds license `i` of `about::entries()` on the Licenses page, or folds it.
    ToggleLicense(usize),
    /// Opens task `n` in place in the list and scrolls it into view (the `#n` links in
    /// sessions).
    OpenTask(u64),
    /// A press on a task slab or on the head of the open task: opens it, or closes it when it
    /// is open.
    ToggleTask(u64),
    /// Esc, the back button, the close button of the open task and Cancel on a draft: drops a
    /// pending confirmation, closes the open task or draft, else goes up one page: from a topic
    /// page to Settings, from Settings to the main page.
    Back,
    ToggleGroup(Group),
    /// Folds or unfolds the sessions panel.
    ToggleSessions,
    /// Folds or unfolds the member list under the header bar.
    ToggleMembers,
    /// Folds or unfolds the Proposal block of the open task.
    ToggleProposal,
    /// Folds or unfolds the Plan block of the open task.
    TogglePlan,

    SelectRoom(RoomId),
    ServerUrl(String),
    RoomName(String),
    RoomKeyInput(String),
    SubmitCreate,
    Created(Result<RoomKey, String>),
    SubmitJoin,
    Joined(Result<RoomId, String>),
    CopyKey,
    /// Leaves the selected room, after the member confirmed it.
    LeaveRoom,
    Left(Result<(), String>),
    /// Forgets the selected room on this machine without asking its server, which does not
    /// answer; after the member confirmed it.
    RemoveRoom,
    NameInput(String),
    SubmitName,
    McpPortInput(String),
    /// Saves the port in the omp section; the MCP server moves to it and every plugin follows.
    SubmitMcpPort,
    /// Shows project `id` of the selected room.
    SelectProject(u64),
    /// Opens the room's settings with the focus on the new project's name.
    OpenNewProject,
    NewProjectName(String),
    CreateProject,
    ProjectCreated(RoomId, Result<Response, String>),
    ProjectNameInput(String),
    RenameProject,
    ProjectRenamed(Result<Response, String>),
    /// Deletes the selected project after the member confirmed it; the server refuses one with
    /// tasks.
    DeleteProject,
    ProjectDeleted(Result<Response, String>),
    /// Moves the open task into project `id`.
    MoveTask(u64),
    TaskMoved(Result<Response, String>),
    ProjectsDirInput(String),
    AddProjectsDir,
    /// Index into the projects folders.
    RemoveProjectsDir(usize),
    /// Opens the projects folders page with the focus on its input.
    OpenProjectsDirs,
    /// Ends the first-start page: saves the projects folders shown on it and opens the main page.
    ConfirmProjectsDirs,
    RepoInput(String),
    AddRepo,
    /// Links a suggested folder to the selected project.
    LinkRepo(PathBuf),
    /// Opens the folder dialog.
    ChooseFolder(FolderFor),
    /// The folder dialog closed; `None` when cancelled.
    FolderChosen(FolderFor, Option<PathBuf>),
    /// Unlinks repo `index` of the selected project.
    RemoveRepo(usize),
    /// Index into the selected project's repos.
    InstallPlugin(usize),
    /// Index into the selected project's repos.
    RemovePlugin(usize),
    /// Index into the selected project's repos.
    UpdatePlugin(usize),
    UpdateAllPlugins,
    AutoUpdatePlugins(bool),
    NotifyHandIns(bool),
    CommitTrailers(bool),

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
    AddWait(u64),
    RemoveWait(u64),
    Decide(u64, Decision),
    Unclaim(u64),
    NoteInput(String),
    AddNote,
    NoteAdded(Result<Response, String>),
    Published(RoomId, Result<(), String>),
    /// Puts a Markdown text of the open task on the clipboard.
    CopyText(TaskText),
    /// The five-minute update check; a server's announcement checks too.
    CheckUpdate,
    /// "Check now": asks GitHub for its latest release, whatever the servers announce.
    CheckLatestUpdate,
    /// `latest` is true when the check asked GitHub's latest release because of "Check now".
    UpdateChecked {
        latest: bool,
        result: Result<Option<update::Release>, String>,
    },
    InstallUpdate,
    UpdateInstalled(Result<semver::Version, String>),
    /// Restarts into the installed update; first asks which widget has the focus.
    RestartForUpdate,
    /// The focused widget and the window's place just before a restart.
    Restart(Option<iced::advanced::widget::Id>, Option<Placement>),
    AutoInstallUpdates(bool),
    AutoRestartUpdates(bool),
    /// Whether the element to focus after a restart is laid out yet.
    ResumeFocus(bool),
}

/// A Markdown text of the open task that a copy button puts on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskText {
    /// Title as `# heading` over the details.
    Details,
    Plan,
    /// Note `i` in note order.
    Note(usize),
}

/// Whether a server answers, with the reason when it does not.
#[derive(Debug, Clone, Default)]
struct ServerStatus {
    online: bool,
    error: Option<String>,
}

/// Which page fills the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Page {
    Main,
    /// The overview of the settings by scope, each topic opening its own page.
    Settings,
    /// About Schwitzer: its license and the open source licenses, opened from Settings.
    Licenses,
    /// The selected room: server, invite key, projects, leave.
    RoomSettings,
    /// The selected project: linked repos and their omp plugin, name, delete.
    ProjectSettings,
    /// All rooms: switch, join, create.
    Rooms,
    /// The omp connection: port, plugins, session files.
    Omp,
    /// The folders searched for repos.
    ProjectsFolders,
    /// This build's version and the self-update.
    Updates,
}

impl Page {
    /// The page Back returns to.
    fn parent(self) -> Option<Page> {
        match self {
            Page::Main => None,
            Page::Settings => Some(Page::Main),
            _ => Some(Page::Settings),
        }
    }
}

/// A danger action waiting for the member to confirm it in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    LeaveRoom,
    RemoveRoom,
    DeleteProject,
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
/// task. `resume.json` stores it as it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Forms {
    server_url: String,
    room_name: String,
    room_key: String,
    name: String,
    /// Empty in a file from a version without it: the restart keeps the saved port.
    #[serde(default)]
    mcp_port: String,
    repo: String,
    new_project: String,
    /// The rename input of the selected project.
    project_name: String,
    new_projects_dir: String,
    proposal_title: String,
    #[serde(with = "resume::editor")]
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
    /// Client events from start-up until the subscription takes them.
    client_backlog: bridge::Backlog,
    data_dir: PathBuf,
    settings: Settings,
    /// The settings as last written (or read), to save only real changes.
    saved: Settings,
    theme: Theme,
    /// The joined rooms in list order, with their last known state.
    rooms: Rooms,
    servers: HashMap<ServerAddr, ServerStatus>,
    selected: Option<RoomId>,
    /// The subfolders of the projects folder, for suggestions; read outside `view`.
    project_dirs: Vec<PathBuf>,
    page: Page,
    /// Whether the first-start page shows: `settings.toml` had no projects folders yet. Until
    /// the member confirms them, `save_settings` leaves `projects_dirs` and `wsl_distros` on
    /// disk as they were, so the page comes back on the next start.
    first_start: bool,
    /// The unfolded entry of the Licenses page.
    license_open: Option<usize>,
    /// The task or draft open in place in the list.
    expanded: Option<Expanded>,
    /// The danger action asked to confirm on a settings page.
    confirm: Option<Confirm>,
    /// Folded groups of the list.
    collapsed: HashSet<Group>,
    /// Whether the sessions panel is unfolded.
    sessions_open: bool,
    /// Whether the member list under the header bar is unfolded.
    members_open: bool,
    /// Whether the open task's Proposal block is folded; every task opens unfolded.
    proposal_folded: bool,
    /// Whether the open task's Plan block is folded; every task opens unfolded.
    plan_folded: bool,
    /// The task whose pick is under way.
    picking: Option<u64>,
    /// Tasks seen approved (Planned → Ready, unclaimed), with the local time, while the
    /// planning omp session claims the build.
    handoffs: HashMap<(RoomId, u64), u64>,
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
    /// The graphics device-lost rule: when the probe is shown and when to restart.
    gpu: gpu::Watch,
}

impl App {
    pub fn boot(resumed: &Resumed) -> (Self, Task<Message>) {
        let data_dir = settings::data_dir();
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            fatal(&format!("cannot create {}: {e}", data_dir.display()));
        }
        let (mut settings, problem) = Settings::load(&data_dir);
        let unreadable = matches!(problem, Some(settings::LoadProblem::Unreadable(_)));
        let saved_before = settings.clone();
        let first_start = !unreadable && settings.projects_dirs.is_none();
        if settings.projects_dirs.is_none() {
            settings.projects_dirs = Some(
                settings::home_dir()
                    .map(|home| settings::default_projects_dirs(&home))
                    .unwrap_or_default(),
            );
        }
        let client = Client::start(&settings::identity_dir(&data_dir), settings.name.clone())
            .unwrap_or_else(|e| fatal(&format!("cannot start the network client: {e}")));
        // Subscribed before the first `add_room`: the server loop starts emitting at once,
        // long before iced runs the subscription.
        let client_backlog: bridge::Backlog = Arc::new(Mutex::new(Some(client.subscribe())));

        let mut notices = Vec::new();
        if let Some(problem) = problem {
            notices.push(problem.notice());
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

        let rooms = Rooms::new(&client);
        let selected = rooms.ids().next();
        let now = now_ms();
        let mut app = Self {
            forms: Forms {
                server_url: settings.last_server.clone().unwrap_or_default(),
                room_name: String::new(),
                room_key: String::new(),
                name: settings.name.clone(),
                mcp_port: settings.mcp_port.to_string(),
                repo: String::new(),
                new_project: String::new(),
                project_name: String::new(),
                new_projects_dir: String::new(),
                proposal_title: String::new(),
                proposal_body: text_editor::Content::new(),
                note: String::new(),
            },
            project_dirs: Vec::new(),
            // An unreadable file is never overwritten at start.
            saved: if unreadable {
                settings.clone()
            } else {
                saved_before
            },
            settings,
            theme: theme::for_mode(iced::theme::Mode::Dark),
            client,
            client_backlog,
            data_dir,
            rooms,
            servers: HashMap::new(),
            selected,
            page: Page::Main,
            first_start,
            license_open: None,
            expanded: None,
            confirm: None,
            collapsed: HashSet::from([Group::Closed]),
            sessions_open: true,
            members_open: false,
            proposal_folded: false,
            plan_folded: false,
            picking: None,
            handoffs: HashMap::new(),
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
            now_ms: now,
            gpu: gpu::Watch::new(now),
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
        let Some(state) = self.selected_state() else {
            return "Schwitzer".to_owned();
        };
        let linked: Vec<u64> = self
            .settings
            .room_project_repos(&state.id)
            .into_iter()
            .filter(|(_, repos)| !repos.is_empty())
            .map(|(project, _)| project)
            .collect();
        match logic::waiting_decisions(state, &linked) {
            0 => format!("{} · Schwitzer", state.name),
            n => format!("({n}) {} · Schwitzer", state.name),
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            Subscription::run_with(
                ClientKey {
                    client: self.client.clone(),
                    backlog: Arc::clone(&self.client_backlog),
                },
                bridge::client_events,
            ),
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
        let body: Element<'_, Message> = if self.first_start {
            settings_view::first_start(self)
        } else {
            match self.page {
                Page::Main => {
                    let mut main = column![chrome::header(self), widgets::hrule()];
                    if let Some(members) = chrome::members(self) {
                        main = main.push(members).push(widgets::hrule());
                    }
                    main.push(self.main()).into()
                }
                Page::Settings => settings_view::view(self),
                Page::Licenses => licenses_view::view(self),
                Page::RoomSettings => settings_view::room_page(self),
                Page::ProjectSettings => settings_view::project_page(self),
                Page::Rooms => settings_view::rooms_page(self),
                Page::Omp => settings_view::omp_page(self),
                Page::ProjectsFolders => settings_view::folders_page(self),
                Page::Updates => settings_view::updates_page(self),
            }
        };
        let window = container(
            page.push(container(body).height(Length::Fill))
                .push(widgets::hrule())
                .push(chrome::status_bar(self)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::ground);
        // The probe is a layer above the window, so dropping it changes neither the layout nor
        // the widget tree below it (no widget loses its state).
        let mut root = stack![window].width(Length::Fill).height(Length::Fill);
        if self.gpu.probe_wanted(self.now_ms) {
            root = root.push(gpu::probe());
        }
        root.into()
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
                        Some(Message::OpenPage(Page::RoomSettings)),
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
                .map(|dir| logic::breakable_path(&logic::folder_label(dir)))
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
                let now = self.now_ms;
                self.handoffs
                    .retain(|_, at| now.saturating_sub(*at) < logic::HANDOFF_MS);
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
                let focus = self.pending_focus();
                if self.gpu.restart_due(self.now_ms) {
                    tracing::warn!("restarting: the graphics device was lost");
                    return Task::batch([focus, restart_task()]);
                }
                focus
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
            Message::CheckUpdate => self.check_update(false),
            Message::CheckLatestUpdate => self.check_update(true),
            Message::UpdateChecked { latest, result } => self.update_checked(latest, result),
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
                restart_task()
            }
            Message::Restart(focus, placement) => {
                self.restart(focus.as_ref(), placement);
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
            Message::OpenSettings => self.open_page(Page::Settings),
            Message::OpenPage(page) => self.open_page(page),
            Message::OpenRoom(id) => {
                self.select(Some(id));
                self.open_page(Page::RoomSettings)
            }
            Message::OpenProject(id) => {
                self.show_project(id);
                self.open_page(Page::ProjectSettings)
            }
            Message::Ask(confirm) => {
                self.confirm = Some(confirm);
                Task::none()
            }
            Message::CancelConfirm => {
                self.confirm = None;
                Task::none()
            }
            Message::OpenLicenses => {
                self.license_open = None;
                self.open_page(Page::Licenses)
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
                if self.choose_project(room, project) {
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
                if self.first_start {
                    self.confirm_projects_dirs();
                    return Task::none();
                }
                if self.confirm.take().is_some() {
                    return Task::none();
                }
                if self.expanded.take().is_some() {
                    self.reset_drafts();
                    return Task::none();
                }
                match self.page.parent() {
                    Some(parent) => self.open_page(parent),
                    None => Task::none(),
                }
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
            Message::ToggleProposal => {
                self.proposal_folded = !self.proposal_folded;
                Task::none()
            }
            Message::TogglePlan => {
                self.plan_folded = !self.plan_folded;
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
            Message::LeaveRoom => {
                self.confirm = None;
                match self.selected {
                    Some(room) => {
                        let client = self.client.clone();
                        call(async move { client.leave_room(room).await }, Message::Left)
                    }
                    None => Task::none(),
                }
            }
            Message::Left(result) => match result {
                Ok(()) => {
                    self.sync_rooms();
                    self.notify("Room left");
                    self.leave_room_page()
                }
                Err(e) => {
                    self.banner = Some(e);
                    Task::none()
                }
            },
            Message::RemoveRoom => {
                self.confirm = None;
                let Some(room) = self.selected else {
                    return Task::none();
                };
                self.client.forget_room(room);
                self.sync_rooms();
                self.notify("Room removed from this app");
                self.leave_room_page()
            }
            Message::NameInput(s) => {
                self.forms.name = s;
                Task::none()
            }
            Message::SubmitName => self.submit_name(),
            Message::McpPortInput(s) => {
                self.forms.mcp_port = s;
                Task::none()
            }
            Message::SubmitMcpPort => {
                self.submit_mcp_port();
                Task::none()
            }
            Message::RepoInput(s) => {
                self.forms.repo = s;
                Task::none()
            }
            Message::AddRepo => match self.link_repo(settings::expand_home(self.forms.repo.trim()))
            {
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
                self.settings.unlink_repo(&room, project.id, index);
                self.save_settings();
                self.publish()
            }
            Message::InstallPlugin(index) => {
                self.write_plugin(index, plugins::Write::Install);
                Task::none()
            }
            Message::RemovePlugin(index) => {
                self.remove_plugin(index);
                Task::none()
            }
            Message::UpdatePlugin(index) => {
                self.write_plugin(index, plugins::Write::Update);
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
            Message::NotifyHandIns(on) => {
                self.settings.notify_hand_ins = on;
                self.save_settings();
                Task::none()
            }
            Message::CommitTrailers(on) => {
                if let (Some(room), Some(project)) =
                    (self.selected, self.selected_project().map(|p| p.id))
                {
                    self.settings.set_commit_trailer(&room, project, on);
                    self.save_settings();
                    self.write_plugin_configs(&room, project);
                }
                Task::none()
            }

            Message::SelectProject(id) => {
                self.show_project(id);
                Task::none()
            }
            Message::OpenNewProject => Task::batch([
                self.open_page(Page::RoomSettings),
                operation::focus("new-project-name"),
            ]),
            Message::OpenProjectsDirs => Task::batch([
                self.open_page(Page::ProjectsFolders),
                operation::focus("projects-dir-input"),
            ]),
            Message::ConfirmProjectsDirs => {
                self.confirm_projects_dirs();
                Task::none()
            }
            Message::NewProjectName(s) => {
                self.forms.new_project = s;
                Task::none()
            }
            Message::CreateProject => {
                let name = caps::cut(self.forms.new_project.trim(), caps::PROJECT_NAME).to_owned();
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
                        self.choose_project(room, project.id);
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
                let name = caps::cut(self.forms.project_name.trim(), caps::PROJECT_NAME).to_owned();
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
            Message::DeleteProject => {
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
            Message::ProjectDeleted(result) => {
                match result {
                    Ok(Response::Ok) => {
                        self.notify("Project deleted");
                        if self.page == Page::ProjectSettings {
                            return self.open_page(Page::RoomSettings);
                        }
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
                if self.add_projects_dir(settings::expand_home(input)) {
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
                self.proposal_folded = false;
                if let Some((_, task)) = self.open() {
                    self.task_draft = Some(TaskDraft {
                        title: task.title.clone(),
                        body: text_editor::Content::with_text(&task.body),
                    });
                }
                Task::batch([operation::focus("task-title"), reveal("open-task")])
            }
            Message::CancelEditTask => {
                self.task_draft = None;
                Task::none()
            }
            Message::CopyText(part) => {
                let Some((_, task)) = self.open() else {
                    return Task::none();
                };
                let (text, notice) = match part {
                    TaskText::Details => (
                        logic::details_markdown(&task.title, &task.body),
                        "Title and details copied",
                    ),
                    TaskText::Plan => (task.plan.clone(), "Plan copied"),
                    TaskText::Note(i) => match task.notes.get(i) {
                        Some(note) => (note.text.clone(), "Note copied"),
                        None => return Task::none(),
                    },
                };
                self.notify(notice);
                iced::clipboard::write(text)
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
                self.plan_folded = false;
                if let Some((_, task)) = self.open() {
                    self.plan_draft = Some(text_editor::Content::with_text(&task.plan));
                }
                reveal_bottom("plan")
            }
            Message::CancelEditPlan => {
                self.plan_draft = None;
                Task::none()
            }
            Message::PlanAction(action) => {
                let Some(draft) = &mut self.plan_draft else {
                    return Task::none();
                };
                // A growing plan editor would push its buttons below the list.
                let edit = action.is_edit();
                draft.perform(action);
                if edit {
                    reveal_bottom("plan")
                } else {
                    Task::none()
                }
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
            Message::AddWait(n) => self.set_waits(|w| w.push(n)),
            Message::RemoveWait(n) => self.set_waits(|w| w.retain(|x| *x != n)),
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
            Message::Unclaim(number) => match self.task_ids(number) {
                Some((room, task)) => fire(self.request(
                    room,
                    Request::Unclaim {
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
        self.rooms.state(self.selected?)
    }

    fn selected_key(&self) -> Option<&RoomKey> {
        self.rooms.key(self.selected?)
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
            .map(|(id, _, state)| RoomChoice {
                id,
                name: logic::room_label(id, state),
            })
            .collect()
    }

    /// The project shown for the selected room: the one last picked, else the first one this
    /// member has linked a repo to, else the first one; `None` while the room has none.
    fn selected_project(&self) -> Option<&Project> {
        let state = self.selected_state()?;
        let chosen = self.settings.chosen_projects.get(&state.id);
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

    /// Remembers `project` as the one shown for `room` and saves the settings; `true` when that
    /// changed.
    fn choose_project(&mut self, room: RoomId, project: u64) -> bool {
        let changed = self.settings.chosen_projects.insert(room, project) != Some(project);
        if changed {
            self.save_settings();
        }
        changed
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

    /// Opens `page` at its top. Drafts and a pending confirmation stay only when it is open
    /// already.
    fn open_page(&mut self, page: Page) -> Task<Message> {
        if self.page != page {
            self.reset_drafts();
            self.confirm = None;
        }
        self.expanded = None;
        self.page = page;
        self.sync_project_form();
        operation::snap_to(pages::BODY_ID, operation::RelativeOffset::START)
    }

    /// After the selected room was left or removed: from its page back to Settings.
    fn leave_room_page(&mut self) -> Task<Message> {
        if self.page == Page::RoomSettings {
            self.open_page(Page::Settings)
        } else {
            Task::none()
        }
    }

    /// Shows project `id` of the selected room.
    fn show_project(&mut self, id: u64) {
        if let Some(room) = self.selected {
            self.choose_project(room, id);
        }
        self.expanded = None;
        self.confirm = None;
        self.reset_drafts();
        self.sync_project_form();
        // A repo cloned a moment ago shows as soon as its project is opened.
        self.read_project_dirs();
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
                path: settings::omp_sessions_dir_in(home),
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

    /// Whether `task` of `room` was approved a moment ago and waits for its build claim.
    fn handing_off(&self, room: RoomId, task: &RoomTask) -> bool {
        logic::handing_off(
            task,
            self.handoffs.get(&(room, task.number)).copied(),
            self.now_ms,
        )
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
        let name = caps::cut(name, caps::ROOM_NAME).to_owned();
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
                Task::batch([self.publish(), self.open_page(Page::Main)])
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
                Task::batch([self.publish(), self.open_page(Page::Main)])
            }
            Err(e) => {
                self.banner = Some(e);
                Task::none()
            }
        }
    }

    fn submit_name(&mut self) -> Task<Message> {
        let name = caps::cut(self.forms.name.trim(), caps::MEMBER_NAME).to_owned();
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

    fn submit_mcp_port(&mut self) {
        let Some(port) = settings::parse_port(&self.forms.mcp_port) else {
            self.banner = Some("Port must be a number from 1 to 65535".to_owned());
            return;
        };
        self.forms.mcp_port = port.to_string();
        if port == self.settings.mcp_port {
            return;
        }
        self.settings.mcp_port = port;
        // "Starting…" until the listener on the new port reports.
        self.mcp = None;
        self.save_settings();
        self.notify("Port saved");
    }

    /// Adds `path` to the projects folders; `false` when it is not a folder on this machine
    /// (with a banner). A folder already listed counts as added.
    fn add_projects_dir(&mut self, path: PathBuf) -> bool {
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

    /// Ends the first-start page: the projects folders and the entered WSL distros are saved
    /// from now on, starting with the list on the page.
    fn confirm_projects_dirs(&mut self) {
        self.first_start = false;
        self.page = Page::Main;
        self.save_settings();
    }

    /// Links `path` to the selected project; `None` when nothing was linked.
    fn link_repo(&mut self, path: PathBuf) -> Option<Task<Message>> {
        let room = self.selected?;
        let project = self.selected_project()?.id;
        let path = match logic::existing_folder(path) {
            Ok(path) => path,
            Err(banner) => {
                self.banner = Some(banner);
                return None;
            }
        };
        self.settings.link_repo(&room, project, path);
        self.save_settings();
        Some(self.publish())
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
                waits_for: Vec::new(),
            }
        };
        call(self.request(room, req), Message::Proposed)
    }

    /// Claims the build of an approved task for this member and hands back the room name and the
    /// claimed task for the prompt.
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
        if !logic::pickable(task) || !task.open_waits(&state.tasks).is_empty() {
            return Task::none();
        }
        let (room, name) = (state.id, state.name.clone());
        self.picking = Some(number);
        let client = self.client.clone();
        call(
            async move {
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

    /// Sends the open task's waits after `edit`, when that changes them.
    fn set_waits(&self, edit: impl FnOnce(&mut Vec<u64>)) -> Task<Message> {
        let Some((state, task)) = self.open() else {
            return Task::none();
        };
        let mut waits_for = task.waits_for.clone();
        edit(&mut waits_for);
        waits_for.sort_unstable();
        waits_for.dedup();
        if waits_for == task.waits_for {
            return Task::none();
        }
        let room = state.id;
        fire(self.request(
            room,
            Request::SetWaits {
                room,
                task: task.number,
                waits_for,
            },
        ))
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
            waits_for: Vec::new(),
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
                    let stale: Vec<RoomId> = self.rooms.ids_on(&server).collect();
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
                let shown = (self.selected == Some(id))
                    .then(|| self.selected_project().map(|p| p.id))
                    .flatten();
                if self.settings.forget_deleted_projects(&id, &state.projects) {
                    self.save_settings();
                }
                let me = self.client.member();
                let notify = self.settings.notify_hand_ins;
                let mut shown_notices: Vec<Task<Message>> = Vec::new();
                match self.rooms.set_state(id, *state) {
                    Some(old) => {
                        if let (Some(old), Some(state)) = (old, self.rooms.state(id)) {
                            let now = now_ms();
                            for n in logic::approved_between(&old, state) {
                                self.handoffs.insert((id, n), now);
                            }
                            if notify {
                                for (work, task) in logic::hand_ins(&old, state, &me) {
                                    let summary = match work {
                                        Work::Plan => format!("Plan handed in · {}", state.name),
                                        Work::Build => format!("Build finished · {}", state.name),
                                    };
                                    let body = format!("#{} {}", task.number, task.title);
                                    shown_notices.push(
                                        Task::future(system_notify::show(summary, body)).discard(),
                                    );
                                }
                            }
                        }
                    }
                    None => self.sync_rooms(),
                }
                if self.selected == Some(id) && self.selected_project().map(|p| p.id) != shown {
                    self.sync_project_form();
                }
                if self.selected.is_none() {
                    self.select(Some(id));
                }
                Task::batch(shown_notices.into_iter().chain([self.publish()]))
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
        self.rooms.reload(&self.client);
        self.published.retain(|room, _| self.rooms.contains(*room));
        if self
            .selected
            .is_none_or(|selected| !self.rooms.contains(selected))
        {
            let first = self.rooms.ids().next();
            self.select(first);
        }
        self.settings
            .retain_rooms(self.rooms.iter().map(|(id, key, _)| (id, key.to_string())));
        self.save_settings();
    }

    fn select(&mut self, room: Option<RoomId>) {
        if self.selected != room {
            self.selected = room;
            self.expanded = None;
            self.confirm = None;
            self.reset_drafts();
            self.sync_project_form();
        }
    }

    /// Drops the open task's drafts and unfolds its Proposal and Plan: the next task opens with
    /// both shown.
    fn reset_drafts(&mut self) {
        self.task_draft = None;
        self.plan_draft = None;
        self.proposal_folded = false;
        self.plan_folded = false;
        self.forms.note.clear();
    }

    /// Writes what the window shows for the new process, starts it and exits (for an installed
    /// update or after the graphics device was lost). The exit skips every destructor, so
    /// nothing of the old process removes what the new one already set up (the remote control
    /// socket).
    fn restart(
        &mut self,
        focus: Option<&iced::advanced::widget::Id>,
        placement: Option<Placement>,
    ) {
        let Some(program) = &self.program else {
            tracing::error!("cannot restart: this program's file is unknown");
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
    /// Changed repos or a changed MCP port also rewrite the omp plugins. On the first-start page
    /// the projects folders and the entered WSL distros are held back.
    fn save_settings(&mut self) {
        // On the first-start page the folders wait for the member's confirmation.
        let on_disk = if self.first_start {
            Cow::Owned(Settings {
                projects_dirs: self.saved.projects_dirs.clone(),
                wsl_distros: self.saved.wsl_distros.clone(),
                ..self.settings.clone()
            })
        } else {
            Cow::Borrowed(&self.settings)
        };
        if *on_disk == self.saved {
            return;
        }
        let repos_changed = on_disk.repos != self.saved.repos;
        let port_changed = on_disk.mcp_port != self.saved.mcp_port;
        match on_disk.save(&self.data_dir) {
            Ok(()) => self.saved = on_disk.into_owned(),
            Err(e) => self.banner = Some(format!("Cannot save settings: {e}")),
        }
        if repos_changed || port_changed {
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
        for (room, _, state) in self.rooms.iter() {
            let Some(state) = state else { continue };
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

/// Asks which widget has the focus and where the window is, then restarts (`Message::Restart`).
fn restart_task() -> Task<Message> {
    iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::find_focused())
        .collect()
        .then(|ids| {
            let focus = ids.into_iter().next();
            resume::placement().map(move |placement| Message::Restart(focus.clone(), placement))
        })
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

/// Scrolls the task list so the bottom of element `id` lies inside it, when the element ends
/// below the list's bottom edge. Runs after the next layout, so it finds what the update opened.
fn reveal_bottom(id: &'static str) -> Task<Message> {
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
            if el.y + el.height - translation.y <= bounds.y + bounds.height {
                return Task::none();
            }
            operation::scroll_to(
                list::LIST_ID,
                AbsoluteOffset {
                    x: 0.0,
                    y: (el.y + el.height + SP_2 - content_bounds.y - bounds.height).max(0.0),
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

/// A start that cannot continue: no data directory, no identity.
fn fatal(message: &str) -> ! {
    eprintln!("schwitzer: {message}");
    std::process::exit(1)
}
