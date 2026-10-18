//! Full-window pages with a back button: New task and Settings.

use iced::widget::{Space, column, container, row, scrollable, text, text_editor, text_input};
use iced::{Alignment, Element, Length};

use super::icons::{Icon, Tone};
use super::widgets::{hrule, icon_button, id_button, muted, text_button};
use super::{App, Message};
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, TS_12, TS_14, TS_20};

/// The widest the body of a page grows in a wide window.
const MAX_WIDTH: f32 = 560.0;

/// A page: header bar with back button and title, scrolling body, optional footer bar.
pub fn frame<'a>(
    id: &'static str,
    title: &'a str,
    body: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let header = container(
        row![
            icon_button(
                "back",
                Icon::Back,
                Tone::Text,
                "Back  (Esc)",
                Some(Message::Back),
                theme::button_ghost,
            ),
            text(title).size(TS_20).font(SEMIBOLD),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    )
    .padding([SP_2, SP_2])
    .width(Length::Fill)
    .style(theme::bar);
    let mut col = column![
        header,
        hrule(),
        scrollable(
            container(container(body).max_width(MAX_WIDTH).width(Length::Fill))
                .center_x(Length::Fill)
                .padding(SP_3)
        )
        .height(Length::Fill)
        .spacing(SP_1)
        .style(theme::scroll),
    ];
    if let Some(footer) = footer {
        col = col.push(hrule()).push(
            container(footer)
                .padding([SP_2, SP_3])
                .width(Length::Fill)
                .style(theme::bar),
        );
    }
    container(col)
        .id(id)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// A block of a page on its own slab.
pub fn section<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(SP_3)
        .width(Length::Fill)
        .style(theme::slab_static)
        .into()
}

/// The New task page: an idea or a proposal for the selected room.
pub fn new_task(app: &App) -> Element<'_, Message> {
    let filled = !app.forms.proposal_title.trim().is_empty();
    let room = app.selected_state().map_or("", |s| s.name.as_str());
    let idea = app.forms.idea;
    let segment = |id: &'static str, label: &'static str, value: bool| {
        id_button(
            id,
            container(text(label).size(TS_12).font(MEDIUM)).padding([SP_1, SP_3]),
            Some(Message::NewIdea(value)),
            theme::button_segment(idea == value),
            Length::Shrink,
        )
    };
    let kind = row![
        segment("new-kind-idea", "Idea", true),
        segment("new-kind-proposal", "Proposal", false),
    ]
    .spacing(SP_1);
    let (hint, title_hint, body_hint, submit) = if idea {
        (
            format!("Ideas land in {room}. Write one up into a proposal when it is ready."),
            "What's the idea?",
            "A few more words (optional)",
            "Add idea",
        )
    } else {
        (
            format!("Proposals land in {room}. Once someone accepts it, an agent writes the plan."),
            "What should be done?",
            "Details, context, links (optional)",
            "Propose",
        )
    };
    let body = column![
        kind,
        muted(hint),
        text_input(title_hint, &app.forms.proposal_title)
            .id("proposal-title")
            .on_input(Message::ProposalTitle)
            .on_submit(Message::Propose)
            .padding(SP_2)
            .size(TS_14)
            .style(theme::input),
        text_editor(&app.forms.proposal_body)
            .id("proposal-body")
            .placeholder(body_hint)
            .min_height(200)
            .padding(SP_2)
            .size(TS_14)
            .on_action(Message::ProposalBody)
            .style(theme::editor),
    ]
    .spacing(SP_3);
    let footer = row![
        Space::new().width(Length::Fill),
        text_button(
            "cancel-new-task",
            "Cancel",
            Some(Message::Back),
            theme::button_secondary
        ),
        text_button(
            "propose",
            submit,
            filled.then_some(Message::Propose),
            theme::button_primary
        ),
    ]
    .spacing(SP_2);
    frame(
        "new-task-page",
        "New task",
        section(body),
        Some(footer.into()),
    )
}
