//! The live agent sessions of the room's members: whose they are, what they are called, which
//! task they work on, how long and how many tokens.

use iced::widget::{Space, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::{RoomState, SessionInfo};

use super::icons::{Icon, Tone};
use super::logic;
use super::widgets::{self, id_button, member_tag, muted};
use super::{App, Message};
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, TS_12, TS_14};

pub fn view<'a>(app: &'a App, state: &'a RoomState) -> Element<'a, Message> {
    let mut rows = column![].spacing(SP_1);
    let mut count = 0;
    for (member, list) in &state.sessions {
        let owner = logic::member_name(&state.members, member);
        let hue = logic::member_hue(member);
        for session in list {
            count += 1;
            rows = rows.push(session_row(app, owner.clone(), hue, session));
        }
    }
    let shares = !app.room_repos().is_empty();
    let mut header = row![
        text("Sessions").size(TS_12).font(SEMIBOLD),
        widgets::pill(count.to_string()),
        Space::new().width(Length::Fill),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if !shares {
        header = header.push(widgets::icon_text_button(
            "share-sessions",
            Icon::FolderPlus,
            Tone::Ink,
            "Share mine",
            Some(Message::OpenSettings),
            theme::button_ghost,
        ));
    }
    let body: Element<'a, Message> = if count == 0 {
        container(
            muted(if shares {
                "No agent is running in the shared repos right now."
            } else {
                "Nobody shares sessions yet. Add your repos for this room in Settings; omp sessions running in them show up here."
            })
            .width(Length::Fill),
        )
        .padding([SP_2, SP_3])
        .into()
    } else {
        scrollable(container(rows).padding([0.0, SP_2]))
            .spacing(SP_1)
            .id("sessions")
            .height(Length::Fill)
            .style(theme::scroll)
            .into()
    };
    column![
        container(header)
            .padding([SP_1, SP_3])
            .height(32)
            .align_y(Alignment::Center),
        body
    ]
    .spacing(SP_1)
    .height(Length::Fill)
    .into()
}

fn session_row<'a>(
    app: &'a App,
    owner: String,
    hue: usize,
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
            member_tag(owner, hue),
            column![text(title).size(TS_14), facts,]
                .spacing(2)
                .width(Length::Fill),
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
    .style(theme::slab_static)
    .into()
}
