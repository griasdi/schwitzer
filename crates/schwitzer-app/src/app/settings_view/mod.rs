//! Settings: an overview of slabs grouped by scope (the selected room and project, this
//! machine, you), each opening the page of its topic; and the first-start page with the
//! projects folders.

mod machine;
mod project;
mod room;

use iced::widget::button;
use iced::widget::text::Wrapping;
use iced::widget::{Space, column, container, row, text, toggler};
use iced::{Alignment, Element, Length, Theme};
use schwitzer_proto::{RoomId, RoomKey};

pub use machine::{folders_page, omp_page, updates_page};
pub use project::project_page;
pub use room::{room_page, rooms_page};

use super::icons::{Icon, Tone, icon};
use super::logic::{self, folder_label};
use super::pages::{frame, section};
use super::widgets::{heading, icon_button, id_button, input, muted, path_text, text_button};
use super::{App, FolderFor, Message, Page};
use crate::omp_plugin::{Coverage, count_outdated};
use crate::settings::tilde;
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, TS_12, TS_14};
use crate::update::{self, UpdateState};
use crate::wsl;

/// The overview: one slab per topic, grouped by what it applies to.
pub fn view(app: &App) -> Element<'_, Message> {
    let mut room = column![].spacing(SP_1);
    if let Some(key) = app.selected_key() {
        room = room.push(room_slab(app, key));
    }
    if let Some(project) = app.selected_project() {
        room = room.push(nav_slab(
            "settings-project",
            Icon::Folder,
            project.name.as_str(),
            project_status(app),
            Message::OpenProject(project.id),
        ));
    }
    room = room.push(if app.rooms.is_empty() {
        nav_slab(
            "settings-rooms",
            Icon::DoorOpen,
            "Join or create a room",
            Status::Notice("You are in no room yet".to_owned()),
            Message::OpenPage(Page::Rooms),
        )
    } else {
        let n = app.rooms.len();
        let rooms = if n == 1 {
            "1 room".to_owned()
        } else {
            format!("{n} rooms")
        };
        nav_slab(
            "settings-rooms",
            Icon::DoorOpen,
            "All rooms",
            Status::Plain(format!("{rooms} · join or create")),
            Message::OpenPage(Page::Rooms),
        )
    });
    let mut room_group = column![].spacing(SP_1);
    if !app.rooms.is_empty() {
        room_group = room_group.push(group_label("Room"));
    }
    room_group = room_group.push(room);

    let mut machine = column![
        nav_slab(
            "settings-omp",
            Icon::Plug,
            "omp",
            omp_status(app),
            Message::OpenPage(Page::Omp),
        ),
        nav_slab(
            "settings-folders",
            Icon::FolderOpen,
            "Projects folders",
            folders_status(app),
            Message::OpenPage(Page::ProjectsFolders),
        ),
    ]
    .spacing(SP_1);
    if update::ENABLED {
        machine = machine.push(nav_slab(
            "settings-updates",
            Icon::Download,
            "Updates",
            updates_status(app),
            Message::OpenPage(Page::Updates),
        ));
    }

    let body = column![
        room_group,
        column![group_label("This machine"), machine].spacing(SP_1),
        column![
            group_label("You"),
            column![
                section(you(app)),
                switch_row(
                    "settings-notify",
                    Switch::Slab(Icon::Bell),
                    "Notify when my agents hand in",
                    "A plan handed in or a build finished, in any room",
                    app.settings.notify_hand_ins,
                    Message::NotifyHandIns,
                ),
            ]
            .spacing(SP_1),
        ]
        .spacing(SP_1),
        nav_slab(
            "settings-about",
            Icon::Info,
            "About Schwitzer",
            Status::Plain(format!("Version {} · licenses", update::CURRENT)),
            Message::OpenLicenses,
        ),
    ]
    .spacing(SP_3);
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

/// The selected room's slab: whether its server answers and who is online.
fn room_slab<'a>(app: &'a App, key: &'a RoomKey) -> Element<'a, Message> {
    let state = app.selected_state();
    let status = if app.servers.get(&key.server).map(|s| s.online) == Some(false) {
        Status::Problem(format!(
            "{}:{} does not answer",
            key.server.host, key.server.port
        ))
    } else if let Some(state) = state {
        let online = state.members.iter().filter(|m| m.online).count();
        Status::Plain(format!(
            "{online} of {} members online",
            state.members.len()
        ))
    } else {
        Status::Plain("Connecting…".to_owned())
    };
    nav_slab(
        "settings-room",
        Icon::Users,
        logic::room_label(RoomId::of(&key.token), state),
        status,
        Message::OpenPage(Page::RoomSettings),
    )
}

/// The selected project's repos on this machine and their omp plugin.
fn project_status(app: &App) -> Status {
    let coverage = Coverage::of(app.project_repos(), &app.plugins);
    let n = coverage.repos;
    if n == 0 {
        Status::Notice("No repo linked on this machine".to_owned())
    } else if coverage.outdated > 0 {
        Status::Notice(format!(
            "omp plugin out of date in {} of {}",
            coverage.outdated,
            repos(n)
        ))
    } else if coverage.missing > 0 {
        Status::Plain(format!(
            "{} · omp plugin missing in {}",
            repos(n),
            coverage.missing
        ))
    } else {
        Status::Plain(format!("{} · omp plugin installed", repos(n)))
    }
}

/// The omp connection, and plugins out of date in any shared repo.
fn omp_status(app: &App) -> Status {
    let Some(mcp) = &app.mcp else {
        return Status::Plain("Starting…".to_owned());
    };
    if let Some(problem) = mcp.problem(app.settings.mcp_port) {
        return Status::Problem(problem);
    }
    let outdated = count_outdated(app.plugins.values());
    if outdated > 0 {
        Status::Notice(format!("omp plugin out of date in {}", repos(outdated)))
    } else {
        Status::Plain(format!("Ready on port {}", app.settings.mcp_port))
    }
}

/// The first projects folder and how many more there are.
fn folders_status(app: &App) -> Status {
    match app.settings.projects_dirs.as_deref().unwrap_or_default() {
        [] => Status::Notice("None yet: add one to get repos suggested".to_owned()),
        [one] => Status::Plain(folder_label(one)),
        [first, rest @ ..] => {
            Status::Plain(format!("{} and {} more", folder_label(first), rest.len()))
        }
    }
}

/// The version and what the update check found.
fn updates_status(app: &App) -> Status {
    match &app.updater {
        UpdateState::Idle => Status::Plain(format!("Version {}", update::CURRENT)),
        UpdateState::Checking => Status::Plain("Checking…".to_owned()),
        UpdateState::UpToDate => Status::Plain(format!("Up to date · {}", update::CURRENT)),
        UpdateState::Failed(_) => Status::Problem("Cannot check for updates".to_owned()),
        UpdateState::Available {
            release,
            error: None,
        } => Status::Notice(format!("Version {} is available", release.version)),
        UpdateState::Available {
            release,
            error: Some(_),
        } => Status::Problem(format!("Version {} did not install", release.version)),
        UpdateState::Installing(release) => {
            Status::Plain(format!("Installing {}…", release.version))
        }
        UpdateState::Installed(version) => {
            Status::Notice(format!("Restart to use version {version}"))
        }
    }
}

/// Your name, shown in every room.
fn you(app: &App) -> Element<'_, Message> {
    let changed = app.forms.name.trim() != app.settings.name && !app.forms.name.trim().is_empty();
    column![
        row![
            input(
                "name-input",
                "Your name",
                &app.forms.name,
                Message::NameInput,
                Message::SubmitName
            )
            .width(Length::Fill),
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

/// The folders searched for repos, to remove from or add to; on the first-start page and on
/// the projects folders page.
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
        rows = rows.push(muted("No projects folder yet."));
    }
    let filled = !app.forms.new_projects_dir.trim().is_empty();
    column![
        heading("Projects folders"),
        rows,
        row![
            input(
                "projects-dir-input",
                "Add a folder, e.g. ~/code",
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

/// The second line of a nav slab: plain, worth a look, or a problem.
enum Status {
    Plain(String),
    Notice(String),
    Problem(String),
}

fn status_line<'a>(status: Status) -> Element<'a, Message> {
    match status {
        Status::Plain(s) => muted(s).wrapping(Wrapping::WordOrGlyph).into(),
        Status::Notice(s) => text(s)
            .size(TS_12)
            .style(theme::text_ink)
            .wrapping(Wrapping::WordOrGlyph)
            .into(),
        Status::Problem(s) => row![
            icon(Icon::Alert, 12.0, Tone::Danger),
            text(s)
                .size(TS_12)
                .style(theme::text_danger)
                .wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(SP_1)
        .align_y(Alignment::Center)
        .into(),
    }
}

/// A raised slab on the overview that opens a page: icon, title, status line, chevron.
fn nav_slab<'a>(
    id: &'static str,
    glyph: Icon,
    title: impl text::IntoFragment<'a>,
    status: Status,
    on_press: Message,
) -> Element<'a, Message> {
    id_button(
        id,
        container(
            row![
                icon(glyph, 16.0, Tone::Muted),
                column![text(title).size(TS_14).font(MEDIUM), status_line(status)]
                    .spacing(2)
                    .width(Length::Fill),
                icon(Icon::ChevronRight, 16.0, Tone::Muted),
            ]
            .spacing(SP_3)
            .align_y(Alignment::Center),
        )
        .padding([SP_2, SP_3]),
        Some(on_press),
        theme::slab,
        Length::Fill,
    )
}

/// How a switch row sits on its page.
enum Switch {
    /// A slab of the overview, led by an icon.
    Slab(Icon),
    /// A flat row inside a section.
    Flat,
}

/// A full-width row with a label, a caption and a switch; a press anywhere on it flips the
/// switch. The switch itself has no handler, so it leaves the press to the row.
fn switch_row<'a>(
    id: &'static str,
    kind: Switch,
    label: &'a str,
    caption: &'a str,
    value: bool,
    message: fn(bool) -> Message,
) -> Element<'a, Message> {
    type Style = fn(&Theme, button::Status) -> button::Style;
    let mut line = row![].spacing(SP_3).align_y(Alignment::Center);
    let (style, padding): (Style, [f32; 2]) = match kind {
        Switch::Slab(glyph) => {
            line = line.push(icon(glyph, 16.0, Tone::Muted));
            (theme::slab, [SP_2, SP_3])
        }
        Switch::Flat => (theme::button_row, [SP_2, SP_2]),
    };
    line = line
        .push(
            column![
                text(label).size(TS_14),
                muted(caption).wrapping(Wrapping::WordOrGlyph)
            ]
            .spacing(2)
            .width(Length::Fill),
        )
        .push(toggler(value).size(20.0).style(theme::switch));
    id_button(
        id,
        container(line).padding(padding),
        Some(message(!value)),
        style,
        Length::Fill,
    )
}

/// The label above a group of slabs.
fn group_label<'a>(label: &'a str) -> Element<'a, Message> {
    container(text(label).size(TS_12).font(SEMIBOLD))
        .padding([0.0, SP_1])
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
