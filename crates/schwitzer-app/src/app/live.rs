//! The live agent sessions of the open project: whose they are, what they are called, which
//! task they work on, how long and how many tokens.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};
use schwitzer_proto::{Member, MemberId, RoomState, SessionInfo};

use super::logic;
use super::widgets::{self, id_button, member_tag_of, muted};
use super::{App, Message};
use crate::theme::{self, MEDIUM, SP_1, SP_2, SP_3, TS_12, TS_14};

/// Height of a session slab: two lines and their padding.
const SLAB_HEIGHT: f32 = 60.0;
/// The panel grows with its sessions up to this many, then scrolls.
const MAX_VISIBLE: f32 = 3.0;

pub fn view<'a>(app: &'a App, state: &'a RoomState, project: u64) -> Element<'a, Message> {
    let mut rows = column![].spacing(SP_1);
    let mut count = 0;
    for (member, list) in &state.sessions {
        for session in list.iter().filter(|s| s.projects.contains(&project)) {
            count += 1;
            rows = rows.push(session_row(app, &state.members, member, session));
        }
    }
    let header = container(widgets::group_head(
        "group-sessions",
        "Sessions",
        count,
        app.sessions_open,
        Message::ToggleSessions,
        None,
    ))
    .padding([SP_1, SP_2]);
    if !app.sessions_open {
        return column![header].into();
    }
    let body: Element<'a, Message> = if count == 0 {
        container(muted("No agent is running in the linked repos right now.").width(Length::Fill))
            .padding([SP_2, SP_3])
            .into()
    } else {
        container(
            scrollable(container(rows).padding(Padding {
                top: 0.0,
                right: SP_2,
                bottom: SP_2,
                left: SP_2,
            }))
            .spacing(SP_1)
            .id("sessions")
            .height(Length::Shrink)
            .style(theme::scroll),
        )
        .max_height(MAX_VISIBLE * SLAB_HEIGHT + (MAX_VISIBLE - 1.0) * SP_1 + SP_2)
        .into()
    };
    column![header, body].into()
}

fn session_row<'a>(
    app: &'a App,
    members: &[Member],
    member: &MemberId,
    session: &'a SessionInfo,
) -> Element<'a, Message> {
    let idle = app.now_ms.saturating_sub(session.active_ms) > logic::IDLE_AFTER_MS;
    let title = if session.title.is_empty() {
        "Untitled session"
    } else {
        session.title.as_str()
    };
    let mut facts = row![muted(&session.project)]
        .spacing(6)
        .align_y(Alignment::Center);
    if let Some(number) = session.task {
        facts = facts.push(id_button(
            format!("session-task-{number}"),
            container(
                text(format!("#{number}"))
                    .size(TS_12)
                    .font(MEDIUM)
                    .style(theme::text_ink),
            )
            .padding([SP_1, SP_1]),
            Some(Message::OpenTask(number)),
            theme::button_ghost,
            Length::Shrink,
        ));
    }
    facts = facts.push(muted(format!("{} tokens", logic::tokens(session.tokens))));
    if idle {
        facts = facts.push(widgets::pill("idle"));
    }
    container(
        row![
            member_tag_of(members, member),
            container(column![text(title).size(TS_14).wrapping(Wrapping::None), facts].spacing(2))
                .width(Length::Fill)
                .clip(true),
            text(logic::runtime(
                app.now_ms.saturating_sub(session.started_ms)
            ))
            .size(TS_12)
            .font(MEDIUM)
            .style(theme::text_muted),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Start),
    )
    .padding([SP_2, SP_3])
    .height(SLAB_HEIGHT)
    .align_y(Alignment::Center)
    .style(theme::slab_static)
    .into()
}
