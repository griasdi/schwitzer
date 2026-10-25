//! The Debug session page of the selected room: start or join, the people in it, this
//! machine's out and in folders, what was sent and received, and the instructions.

use iced::widget::text::Wrapping;
use iced::widget::{Column, column, row, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::{DebugSession, MemberId, RoomState, caps};

use super::icons::{Icon, Tone};
use super::logic;
use super::pages::{frame, section};
use super::widgets::{
    confirm_row, heading, icon_button, icon_text_button, member_tag, muted, path_text, pill,
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
    row(members
        .iter()
        .map(|m| member_tag(logic::member_name(&state.members, m), logic::member_hue(m))))
    .spacing(SP_1)
    .wrap()
    .into()
}

fn joined<'a>(
    app: &'a App,
    state: &'a RoomState,
    session: &'a DebugSession,
    me: MemberId,
) -> Element<'a, Message> {
    let master = session.master == me;
    let mut people = column![heading("People")].spacing(SP_2);
    for m in &session.members {
        let mut line = row![member_tag(
            logic::member_name(&state.members, m),
            logic::member_hue(m)
        )]
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

    let dirs = debug::SessionDirs::new(&app.data_dir, session.id);
    let out_dir = dirs.out();
    let out = out_dir.display().to_string();
    let inn = dirs.inbox().display().to_string();
    let example = out_dir.join("app.log").display().to_string();
    let folders = column![
        heading("Your folders"),
        folder("Out: files here go to everyone", out, "debug-copy-out"),
        folder("In: what the others send", inn, "debug-copy-in"),
        muted(format!(
            "Example: your-app 2>&1 | tee \"{}\"",
            logic::breakable_path(&example)
        ))
        .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(SP_2);

    let traffic = app.debug_traffic.get(&state.id);
    let counts = |title: &'a str, map: Option<&std::collections::BTreeMap<String, u64>>| {
        let mut col = column![heading(title)].spacing(SP_1);
        match map.filter(|m| !m.is_empty()) {
            None => col = col.push(muted("Nothing yet")),
            Some(map) => {
                for (name, n) in map {
                    col = col.push(
                        text(format!("{name} · {}", logic::bytes(*n)))
                            .size(TS_14)
                            .wrapping(Wrapping::WordOrGlyph),
                    );
                }
            }
        }
        col
    };
    let mut sending = counts("Sending", traffic.map(|t| &t.sent));
    if let Some(e) = traffic.and_then(|t| t.error.as_ref()) {
        sending = sending.push(
            text(e)
                .size(TS_14)
                .style(theme::text_danger)
                .wrapping(Wrapping::WordOrGlyph),
        );
    }
    let mut receiving = counts("Receiving", traffic.map(|t| &t.received));
    if traffic.is_some_and(|t| t.lost) {
        receiving = receiving.push(muted("Some received data was lost"));
    }

    let mut instructions =
        column![heading(if master { "Sent" } else { "Instructions" })].spacing(SP_2);
    let inbox = app.client.debug_inbox(state.id);
    let entries: Vec<_> = inbox.iter().filter(|e| master || e.for_me).collect();
    if entries.is_empty() {
        instructions = instructions.push(muted("Nothing yet"));
    }
    for entry in entries {
        let i = &entry.instruction;
        let mut who = row![member_tag(
            logic::member_name(&state.members, &i.from),
            logic::member_hue(&i.from)
        )]
        .spacing(SP_1)
        .align_y(Alignment::Center);
        if master {
            who = who.push(muted("to"));
            for to in &i.to {
                who = who.push(member_tag(
                    logic::member_name(&state.members, to),
                    logic::member_hue(to),
                ));
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
                icon_button(
                    format!("debug-copy-instruction-{}", i.id),
                    Icon::Copy,
                    Tone::Text,
                    "Copy",
                    Some(Message::CopyDebug(i.text.clone())),
                    theme::button_ghost,
                ),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Start),
        );
    }

    let master_name = logic::member_name(&state.members, &session.master);
    let omp = if master {
        in_omp(vec![
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
        ])
    } else {
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
    };

    Column::with_children([
        section(people),
        omp,
        section(folders),
        section(sending),
        section(receiving),
        section(instructions),
    ])
    .spacing(SP_3)
    .into()
}

/// A folder with its label and a copy button.
fn folder<'a>(label: &'a str, path: String, id: &'static str) -> Element<'a, Message> {
    column![
        muted(label),
        row![
            path_text(&path),
            icon_button(
                id,
                Icon::Copy,
                Tone::Text,
                "Copy",
                Some(Message::CopyDebug(path.clone())),
                theme::button_ghost,
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    ]
    .spacing(2)
    .into()
}

/// An omp command: what it does, then the command with a copy button.
fn command<'a>(label: String, command: &'static str, id: &'static str) -> Element<'a, Message> {
    column![
        muted(label).wrapping(Wrapping::WordOrGlyph),
        row![
            text(command).size(TS_14).font(MEDIUM).width(Length::Fill),
            icon_button(
                id,
                Icon::Copy,
                Tone::Text,
                "Copy",
                Some(Message::CopyDebug(command.to_owned())),
                theme::button_ghost,
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    ]
    .spacing(2)
    .into()
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
