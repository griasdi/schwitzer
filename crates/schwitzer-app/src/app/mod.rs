//! The application: state, messages and `update`. The views live in the submodules.

mod chrome;
mod debug_session;
mod debug_view;
mod gpu;
mod icons;
mod licenses_view;
mod list;
mod live;
pub mod logic;
mod main_view;
mod pages;
mod plugins;
mod projects;
mod resume;
mod rooms;
mod settings_view;
mod sticky;
mod task_view;
mod tasks;
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
    SessionInfo, Task as RoomTask, Work, caps, now_ms,
};
use serde::{Deserialize, Serialize};

use self::chrome::RoomChoice;
use self::logic::{Group, ServerStatus};
use self::rooms::Rooms;
use crate::bridge::{self, ClientKey, call};
use crate::debug;
use crate::mcp::{self, McpStatus};
use crate::omp_plugin::PluginState;
use crate::sessions::{self, LocalSession};
use crate::settings::{self, Settings};
use crate::system_notify;
use crate::theme::{self, SP_2};
use crate::update::{self, UpdateState};
use crate::wsl;

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
    /// Starts a debug session in the selected room.
    DebugStart,
    /// Joins the selected room's debug session.
    DebugJoin,
    /// Leaves the selected room's debug session; the master ends it, after confirming.
    DebugLeave,
    /// A debug worker's counts changed.
    DebugTraffic(debug::Traffic),
    /// Puts a text of the debug page on the clipboard.
    CopyDebug(String),
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
    /// Closes the plugin prompt without installing.
    SkipPluginPrompt,
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
    QuickIdea(text_editor::Action),
    /// Adds the quick idea field's text as an idea.
    NoteIdea,
    IdeaNoted(Result<Response, String>),
    /// Ctrl+L: opens the main page and focuses the quick idea field.
    FocusIdea,
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
    /// `latest` is true when the check asked GitHub's latest release because of "Check now";
    /// `channel` is the one the check followed.
    UpdateChecked {
        latest: bool,
        channel: schwitzer_release::Channel,
        result: Result<Option<update::Release>, String>,
    },
    InstallUpdate,
    UpdateInstalled(Result<semver::Version, String>),
    /// Restarts into the installed update; first asks which widget has the focus.
    RestartForUpdate,
    /// The focused widget and the window's place just before a restart.
    Restart(Option<iced::advanced::widget::Id>, Option<Placement>),
    AutoInstallUpdates(bool),
    /// The member picked an update channel on the Updates page.
    UpdateChannel(schwitzer_release::Channel),
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

/// Which page fills the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Page {
    Main,
    /// Choosing the projects folders: `settings.toml` had no projects folders yet. Until the
    /// member confirms them, `save_settings` leaves `projects_dirs` and `wsl_distros` on disk
    /// as they were, so the page comes back on the next start. Nothing but the confirmation
    /// leaves it.
    FirstStart,
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
    /// The debug session of the selected room.
    Debug,
}

impl Page {
    /// The page Back returns to.
    fn parent(self) -> Option<Page> {
        match self {
            Page::Main | Page::FirstStart => None,
            Page::Settings | Page::Debug => Some(Page::Main),
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
    EndDebug,
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
    /// The quick idea field above the sessions.
    #[serde(default, with = "resume::editor")]
    quick_idea: text_editor::Content,
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
    /// The text of the quick idea being sent; Enter does nothing while it is under way.
    noting_idea: Option<String>,
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
    /// The repo just linked from the main page, whose plugin prompt is open until the plugin is
    /// installed or the member presses "Not now".
    plugin_prompt: Option<PathBuf>,
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
    /// What each joined debug session's worker moved.
    debug_traffic: HashMap<RoomId, debug::Traffic>,
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
                quick_idea: text_editor::Content::new(),
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
            page: if first_start {
                Page::FirstStart
            } else {
                Page::Main
            },
            license_open: None,
            expanded: None,
            confirm: None,
            collapsed: HashSet::from([Group::Closed]),
            sessions_open: true,
            members_open: false,
            proposal_folded: false,
            plan_folded: false,
            picking: None,
            noting_idea: None,
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
            plugin_prompt: None,
            program: update::Program::current(),
            updater: UpdateState::Idle,
            update_failed: None,
            releases: HashMap::new(),
            resume_focus: None,
            now_ms: now,
            gpu: gpu::Watch::new(now),
            debug_traffic: HashMap::new(),
        };
        app.read_project_dirs();
        if update::ENABLED
            && let Some(program) = &app.program
        {
            schwitzer_release::clean_leftovers(&program.path);
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
                mcp::subscription(
                    self.client.clone(),
                    self.settings.mcp_port,
                    token.clone(),
                    self.data_dir.clone(),
                )
                .map(Message::Mcp),
            );
        }
        subs.extend(self.debug_subscriptions());
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
            Page::FirstStart => settings_view::first_start(self),
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
            Page::Debug => debug_view::page(self),
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

    /// Whether the main page shows the task list: a project is selected, it has a linked repo
    /// and the plugin prompt is not open.
    fn shows_list(&self) -> bool {
        self.selected_project().is_some()
            && !self.project_repos().is_empty()
            && self.prompted_plugin().is_none()
    }

    /// Brings up the main page for Ctrl+N and Ctrl+L, which type into its list; false when it
    /// shows no list (first start, no project, no repo, the plugin prompt).
    fn show_main_for_input(&mut self) -> bool {
        if self.page == Page::FirstStart || !self.shows_list() {
            return false;
        }
        if self.page != Page::Main {
            self.page = Page::Main;
            self.reset_drafts();
        }
        true
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
            Message::UpdateChecked {
                latest,
                channel,
                result,
            } => self.update_checked(latest, channel, result),
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
            Message::UpdateChannel(channel) => self.set_update_channel(channel),
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
                if !self.show_main_for_input() {
                    return Task::none();
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
            Message::FocusIdea => {
                if !self.show_main_for_input() {
                    return Task::none();
                }
                operation::focus("quick-idea")
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
                if self.page == Page::FirstStart {
                    return Task::none();
                }
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
                if self.page == Page::FirstStart {
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
            Message::DebugStart => self.debug_start(),
            Message::DebugJoin => self.debug_join(),
            Message::DebugLeave => self.debug_leave(),
            Message::DebugTraffic(traffic) => {
                self.on_debug_traffic(traffic);
                Task::none()
            }
            Message::CopyDebug(text) => self.copy_debug(text),
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
            Message::AddRepo => self.add_repo(),
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
            Message::RemoveRepo(index) => self.remove_repo(index),
            Message::InstallPlugin(index) => {
                self.write_plugin(index, plugins::Write::Install);
                // Installed: the prompt is done for good, also once the plugin is removed again.
                if self
                    .plugin_prompt
                    .as_ref()
                    .is_some_and(|repo| self.plugins.contains_key(repo))
                {
                    self.plugin_prompt = None;
                }
                Task::none()
            }
            Message::SkipPluginPrompt => {
                self.plugin_prompt = None;
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
            Message::CreateProject => self.create_project(),
            Message::ProjectCreated(room, result) => self.project_created(room, result),
            Message::ProjectNameInput(s) => {
                self.forms.project_name = s;
                Task::none()
            }
            Message::RenameProject => self.rename_project(),
            Message::ProjectRenamed(result) => self.project_renamed(result),
            Message::DeleteProject => self.delete_project(),
            Message::ProjectDeleted(result) => self.project_deleted(result),
            Message::MoveTask(project) => self.move_task(project),
            Message::TaskMoved(result) => self.task_moved(result),
            Message::ProjectsDirInput(s) => {
                self.forms.new_projects_dir = s;
                Task::none()
            }
            Message::AddProjectsDir => {
                self.submit_projects_dir();
                Task::none()
            }
            Message::RemoveProjectsDir(index) => {
                self.remove_projects_dir(index);
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
            Message::Proposed(result) => self.proposed(result),
            Message::QuickIdea(action) => {
                self.forms.quick_idea.perform(action);
                Task::none()
            }
            Message::NoteIdea => self.note_idea(),
            Message::IdeaNoted(result) => self.idea_noted(result),
            Message::Pick(number) => self.pick(number),
            Message::Picked(result) => self.picked(result),

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
            Message::TaskSaved(result) => self.task_saved(result),
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
            Message::PlanSaved(result) => self.plan_saved(result),
            Message::SetPriority(priority) => self.set_priority(priority),
            Message::AddWait(n) => self.set_waits(|w| w.push(n)),
            Message::RemoveWait(n) => self.set_waits(|w| w.retain(|x| *x != n)),
            Message::Decide(number, decision) => self.decide(number, decision),
            Message::Unclaim(number) => self.unclaim(number),
            Message::NoteInput(s) => {
                self.forms.note = s;
                Task::none()
            }
            Message::AddNote => self.add_note(),
            Message::NoteAdded(result) => self.note_added(result),
        }
    }

    // ---- lookups ------------------------------------------------------------------------

    fn selected_state(&self) -> Option<&RoomState> {
        self.rooms.state(self.selected?)
    }

    fn selected_key(&self) -> Option<&RoomKey> {
        self.rooms.key(self.selected?)
    }

    /// Whether `server` answers; `None` before it said either.
    fn server_online(&self, server: &ServerAddr) -> Option<bool> {
        self.servers.get(server).map(|s| s.online)
    }

    /// Whether the selected room's server answers; `None` before it said either.
    fn selected_server_online(&self) -> Option<bool> {
        self.server_online(&self.selected_key()?.server)
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
    /// already. The first-start page stays until its confirmation.
    fn open_page(&mut self, page: Page) -> Task<Message> {
        if self.page == Page::FirstStart {
            return Task::none();
        }
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

    /// Ends the first-start page: the projects folders and the entered WSL distros are saved
    /// from now on, starting with the list on the page.
    fn confirm_projects_dirs(&mut self) {
        self.page = Page::Main;
        self.save_settings();
    }

    /// Index in [`Self::project_repos`] of the repo whose plugin prompt is open: linked to the
    /// selected project and still without the plugin.
    fn prompted_plugin(&self) -> Option<usize> {
        let repo = self.plugin_prompt.as_ref()?;
        if self.plugins.contains_key(repo) {
            return None;
        }
        self.project_repos().iter().position(|r| r == repo)
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
                        self.banner = Some(format!("{}: {e}", server.authority()));
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
                let mut debug_notice = None;
                match self.rooms.set_state(id, *state) {
                    Some(old) => {
                        if let (Some(old), Some(state)) = (old, self.rooms.state(id)) {
                            debug_notice = logic::debug_change(&old, state, &me);
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
                self.on_debug_room_change(debug_notice);
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
            ClientEvent::DebugInbox { room, fresh } => {
                self.on_debug_inbox(room, fresh);
                Task::none()
            }
            // The bridge keeps log bytes out of the message loop; the debug worker reads them.
            ClientEvent::DebugLog { .. } => Task::none(),
            ClientEvent::Release { server, version } => {
                let Ok(version) = version.parse::<semver::Version>() else {
                    tracing::warn!(
                        "{} announced an unreadable version {version:?}",
                        server.authority()
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
        self.prune_debug_traffic();
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
        let on_disk = if self.page == Page::FirstStart {
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
        Key::Character("l") if modifiers.command() => Some(Message::FocusIdea),
        Key::Character(",") if modifiers.command() => Some(Message::OpenSettings),
        _ => None,
    }
}

/// Where the task list lies, as the selector found it.
struct ListView {
    bounds: iced::Rectangle,
    content_bounds: iced::Rectangle,
    translation: iced::Vector,
}

/// Scrolls the task list to the offset `offset` returns for the element with widget id `id`,
/// given the list and the element's bounds; `None` leaves the list where it is. Runs after the
/// next layout, so it finds an element the same update opened.
fn reveal_by(
    id: &'static str,
    offset: fn(&ListView, iced::Rectangle) -> Option<f32>,
) -> Task<Message> {
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
        let list = ListView {
            bounds,
            content_bounds,
            translation,
        };
        selector::find(selector::id(id)).then(move |target| {
            let Some(target) = target else {
                return Task::none();
            };
            match offset(&list, target.bounds()) {
                Some(y) => operation::scroll_to(list::LIST_ID, AbsoluteOffset { x: 0.0, y }),
                None => Task::none(),
            }
        })
    })
}

/// Scrolls the task list so the element with widget id `id` shows: when its top is above the
/// list, or its first list-height is below it, its top goes to the top of the list.
fn reveal(id: &'static str) -> Task<Message> {
    reveal_by(id, |list, el| {
        let top = el.y - list.translation.y;
        let bottom = top + el.height.min(list.bounds.height);
        if top >= list.bounds.y && bottom <= list.bounds.y + list.bounds.height {
            return None;
        }
        Some((el.y - list.content_bounds.y - SP_2).max(0.0))
    })
}

/// Scrolls the task list so the bottom of element `id` lies inside it, when the element ends
/// below the list's bottom edge.
fn reveal_bottom(id: &'static str) -> Task<Message> {
    reveal_by(id, |list, el| {
        if el.y + el.height - list.translation.y <= list.bounds.y + list.bounds.height {
            return None;
        }
        Some((el.y + el.height + SP_2 - list.content_bounds.y - list.bounds.height).max(0.0))
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
    tracing::error!("{message}");
    std::process::exit(1)
}
