//! The Debug session page of the selected room: start or join, the people in it, this
//! machine's out and in folders, what was sent and received, and the instructions.

use std::collections::BTreeMap;

use iced::widget::text::Wrapping;
use iced::widget::{Column, column, row, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::{DebugSession, MemberId, RoomState, caps};

use super::icons::{Icon, Tone};
use super::logic;
use super::pages::{frame, section};
use super::widgets::{
    confirm_row, heading, icon_button, icon_text_button, member_tag_of, muted, path_text, pill,
    text_button,
};
use super::{App, Confirm, Message};
use crate::debug;
use crate::theme::{self, MEDIUM, SP_1, SP_2, SP_3, TS_14};

/// Bytes of an instruction shown on the page; the copy button takes all of it.
const INSTRUCTION_PREVIEW: usize = 300;

/// The omp command that starts, joins or picks up the debug session.
const OMP_DEBUG: &str = "/schwitzer debug";
/// The omp command that leaves the session; the master's ends it for everyone.
const OMP_DEBUG_STOP: &str = "/schwitzer debug stop";

pub fn page(app: &App) -> Element<'_, Message> {
    let Some(state) = app.selected_state() else {
        return frame(
            "debug-page",
            "Debug session",
            section(muted("Choose a room first.")),
            None,
        );
    };
    let me = app.client.member();
    let (body, footer) = match &state.debug {
        None => (
            column![
                section(muted(
                    "Test a networked app together. Everyone who joins streams the files in \
                     their out folder to the others. The master's agent can send instructions; \
                     on each machine a human decides whether their agent runs one.",
                )),
                in_omp(vec![command(
                    "Or start it from an omp session in one of this room's repos, with you as \
                     master:"
                        .to_owned(),
                    OMP_DEBUG,
                    "debug-copy-command",
                )]),
            ]
            .spacing(SP_3)
            .into(),
            text_button(
                "debug-start",
                "Start debug session",
                Some(Message::DebugStart),
                theme::button_primary,
            ),
        ),
        Some(session) if !session.members.contains(&me) => {
            let master = logic::member_name(&state.members, &session.master);
            (
                column![
                    section(
                        column![
                            text(format!("{master} started a debug session")).size(TS_14),
                            tags(state, &session.members),
                        ]
                        .spacing(SP_2),
                    ),
                    in_omp(vec![command(
                        format!(
                            "Or join from omp; instructions from {master} then wait there for \
                             your OK:"
                        ),
                        OMP_DEBUG,
                        "debug-copy-command",
                    )]),
                ]
                .spacing(SP_3)
                .into(),
                text_button(
                    "debug-join",
                    "Join",
                    Some(Message::DebugJoin),
                    theme::button_primary,
                ),
            )
        }
        Some(session) => (joined(app, state, session, me), leave(app, session, me)),
    };
    frame("debug-page", "Debug session", body, Some(footer))
}

/// The members as name tags.
fn tags<'a>(state: &RoomState, members: &[MemberId]) -> Element<'a, Message> {
    row(members.iter().map(|m| member_tag_of(&state.members, m)))
        .spacing(SP_1)
        .wrap()
        .into()
}

/// A session this member is in: the people, the omp commands, the folders, what was sent and
/// received, and the instructions.
fn joined<'a>(
    app: &'a App,
    state: &'a RoomState,
    session: &'a DebugSession,
    me: MemberId,
) -> Element<'a, Message> {
    let traffic_now = app.debug_traffic.get(&state.id);
    let error: Option<Element<'a, Message>> = traffic_now.and_then(|t| t.error.as_ref()).map(|e| {
        text(e)
            .size(TS_14)
            .style(theme::text_danger)
            .wrapping(Wrapping::WordOrGlyph)
            .into()
    });
    let lost: Option<Element<'a, Message>> = traffic_now
        .is_some_and(|t| t.lost)
        .then(|| muted("Some received data was lost").into());
    Column::with_children([
        section(people(state, session)),
        omp_commands(state, session, session.master == me),
        section(folders(app, session)),
        traffic("Sending", traffic_now.map(|t| &t.sent), error),
        traffic("Receiving", traffic_now.map(|t| &t.received), lost),
        section(instructions(app, state, session.master == me)),
    ])
    .spacing(SP_3)
    .into()
}

/// Everyone in the session, with who is master and who is offline.
fn people<'a>(state: &RoomState, session: &DebugSession) -> Column<'a, Message> {
    let mut people = column![heading("People")].spacing(SP_2);
    for m in &session.members {
        let mut line = row![member_tag_of(&state.members, m)]
            .spacing(SP_1)
            .align_y(Alignment::Center);
        if *m == session.master {
            line = line.push(pill("master"));
        }
        if state.members.iter().any(|x| x.id == *m && !x.online) {
            line = line.push(pill("offline"));
        }
        people = people.push(line);
    }
    people
}

/// The omp commands of the session: the master's start and end, a participant's join and leave.
fn omp_commands<'a>(
    state: &RoomState,
    session: &DebugSession,
    master: bool,
) -> Element<'a, Message> {
    if master {
        return in_omp(vec![
            command(
                "Run in omp so your agent gets the folders and can send instructions:".to_owned(),
                OMP_DEBUG,
                "debug-copy-command",
            ),
            command(
                "Ends the session for everyone:".to_owned(),
                OMP_DEBUG_STOP,
                "debug-copy-stop",
            ),
        ]);
    }
    let master_name = logic::member_name(&state.members, &session.master);
    in_omp(vec![
        command(
            format!("Run in omp to get {master_name}'s instructions there for your OK:"),
            OMP_DEBUG,
            "debug-copy-command",
        ),
        command(
            "Leaves the session:".to_owned(),
            OMP_DEBUG_STOP,
            "debug-copy-stop",
        ),
    ])
}

/// This machine's out and in folders of the session, each with a copy button.
fn folders<'a>(app: &App, session: &DebugSession) -> Column<'a, Message> {
    let dirs = debug::SessionDirs::new(&app.data_dir, session.id);
    let out_dir = dirs.out();
    let out = out_dir.display().to_string();
    let inn = dirs.inbox().display().to_string();
    let example = out_dir.join("app.log").display().to_string();
    column![
        heading("Your folders"),
        copy_line(
            muted("Out: files here go to everyone"),
            path_text(&out),
            out.clone(),
            "debug-copy-out",
        ),
        copy_line(
            muted("In: what the others send"),
            path_text(&inn),
            inn.clone(),
            "debug-copy-in",
        ),
        muted(format!(
            "Example: your-app 2>&1 | tee \"{}\"",
            logic::breakable_path(&example)
        ))
        .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(SP_2)
}

/// What one worker direction moved, per file, then `note` (an error, a loss); the slab of the
/// Sending and the Receiving block.
fn traffic<'a>(
    title: &'static str,
    files: Option<&BTreeMap<String, u64>>,
    note: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut col = column![heading(title)].spacing(SP_1);
    match files.filter(|files| !files.is_empty()) {
        None => col = col.push(muted("Nothing yet")),
        Some(files) => {
            for (name, n) in files {
                col = col.push(
                    text(format!("{name} · {}", logic::bytes(*n)))
                        .size(TS_14)
                        .wrapping(Wrapping::WordOrGlyph),
                );
            }
        }
    }
    if let Some(note) = note {
        col = col.push(note);
    }
    section(col)
}

/// The instructions of the session: all that were sent for the master, those addressed to
/// this member otherwise.
fn instructions<'a>(app: &App, state: &RoomState, master: bool) -> Column<'a, Message> {
    let mut instructions =
        column![heading(if master { "Sent" } else { "Instructions" })].spacing(SP_2);
    let inbox = app.client.debug_inbox(state.id);
    let entries: Vec<_> = inbox.iter().filter(|e| master || e.for_me).collect();
    if entries.is_empty() {
        instructions = instructions.push(muted("Nothing yet"));
    }
    for entry in entries {
        let i = &entry.instruction;
        let mut who = row![member_tag_of(&state.members, &i.from)]
            .spacing(SP_1)
            .align_y(Alignment::Center);
        if master {
            who = who.push(muted("to"));
            for to in &i.to {
                who = who.push(member_tag_of(&state.members, to));
            }
        } else {
            who = who.push(pill(if entry.taken { "in omp" } else { "waiting" }));
        }
        let shown = caps::cut(&i.text, INSTRUCTION_PREVIEW);
        let shown = if shown.len() < i.text.len() {
            format!("{shown}…")
        } else {
            shown.to_owned()
        };
        instructions = instructions.push(
            row![
                column![
                    who.wrap(),
                    text(format!("#{} {shown}", i.id))
                        .size(TS_14)
                        .width(Length::Fill)
                        .wrapping(Wrapping::WordOrGlyph)
                ]
                .spacing(SP_1)
                .width(Length::Fill),
                copy_button(format!("debug-copy-instruction-{}", i.id), i.text.clone()),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Start),
        );
    }
    instructions
}

/// A muted label over a line with `content` and a button that copies `copied`.
fn copy_line<'a>(
    label: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    copied: String,
    id: &'static str,
) -> Element<'a, Message> {
    column![
        label.into(),
        row![content.into(), copy_button(id, copied)]
            .spacing(SP_2)
            .align_y(Alignment::Center),
    ]
    .spacing(2)
    .into()
}

/// The icon button that puts `copied` on the clipboard.
fn copy_button<'a>(id: impl Into<String>, copied: String) -> Element<'a, Message> {
    icon_button(
        id,
        Icon::Copy,
        Tone::Text,
        "Copy",
        Some(Message::CopyDebug(copied)),
        theme::button_ghost,
    )
}

/// An omp command: what it does, then the command with a copy button.
fn command<'a>(label: String, command: &'static str, id: &'static str) -> Element<'a, Message> {
    copy_line(
        muted(label).wrapping(Wrapping::WordOrGlyph),
        text(command).size(TS_14).font(MEDIUM).width(Length::Fill),
        command.to_owned(),
        id,
    )
}

/// The "In omp" slab with its commands.
fn in_omp<'a>(commands: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut col = column![heading("In omp")].spacing(SP_2);
    for c in commands {
        col = col.push(c);
    }
    section(col)
}

/// The participant's Leave, or the master's End session with its confirmation.
fn leave<'a>(app: &App, session: &DebugSession, me: MemberId) -> Element<'a, Message> {
    if session.master != me {
        return text_button(
            "debug-leave",
            "Leave",
            Some(Message::DebugLeave),
            theme::button_secondary,
        );
    }
    if app.confirm == Some(Confirm::EndDebug) {
        return confirm_row(
            "End the debug session for everyone?".to_owned(),
            "confirm-end-debug",
            Icon::Close,
            "End",
            Message::DebugLeave,
        );
    }
    icon_text_button(
        "debug-end",
        Icon::Close,
        Tone::Danger,
        "End session",
        Some(Message::Ask(Confirm::EndDebug)),
        theme::button_danger,
    )
}
