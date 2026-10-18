//! Dialogs over the list: the modal layer, the shared frame and the new-task dialog.

use iced::widget::{
    Space, column, container, mouse_area, opaque, row, scrollable, stack, text, text_editor,
    text_input,
};
use iced::{Alignment, Element, Length, Padding};

use super::icons::{Icon, Tone};
use super::widgets::{icon_button, id_button, muted, text_button};
use super::{App, Message};
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, SP_4, TS_12, TS_14, TS_20};

/// The widest a dialog grows in a wide window.
const MAX_WIDTH: f32 = 560.0;

/// `dialog` over `base`: the base is dimmed and takes no input; a click on the dim closes.
pub fn modal<'a>(
    base: impl Into<Element<'a, Message>>,
    dialog: Element<'a, Message>,
) -> Element<'a, Message> {
    stack![
        base.into(),
        opaque(
            mouse_area(
                container(opaque(dialog))
                    .center(Length::Fill)
                    .padding(SP_3)
                    .style(theme::scrim)
            )
            .on_press(Message::CloseDialog)
        )
    ]
    .into()
}

/// A dialog card: `header` with a close button, `body` scrolling below, `footer` pinned under
/// it. `tall` dialogs take the full height, the others their content's.
pub fn frame<'a>(
    id: &'static str,
    header: Element<'a, Message>,
    body: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
    tall: bool,
) -> Element<'a, Message> {
    let top = row![
        container(header).width(Length::Fill),
        icon_button(
            "close-dialog",
            Icon::Close,
            Tone::Muted,
            "Close  (Esc)",
            Some(Message::CloseDialog),
            theme::button_ghost,
        ),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    let mut col = column![
        container(top).padding(Padding {
            top: SP_2,
            right: SP_2,
            bottom: 0.0,
            left: SP_4,
        }),
        scrollable(container(body).padding(Padding {
            top: SP_2,
            right: SP_4,
            bottom: SP_4,
            left: SP_4,
        }))
        .height(if tall { Length::Fill } else { Length::Shrink })
        .style(theme::scroll),
    ];
    if let Some(footer) = footer {
        col = col.push(container(footer).padding(Padding {
            top: 0.0,
            right: SP_4,
            bottom: SP_4,
            left: SP_4,
        }));
    }
    container(col)
        .id(id)
        .max_width(MAX_WIDTH)
        .width(Length::Fill)
        .height(if tall { Length::Fill } else { Length::Shrink })
        .style(theme::dialog)
        .into()
}

/// A dialog title.
pub fn title<'a>(label: &'a str) -> Element<'a, Message> {
    text(label).size(TS_20).font(SEMIBOLD).into()
}

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
            .min_height(140)
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
            Some(Message::CloseDialog),
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
        "new-task-dialog",
        title("New task"),
        body.into(),
        Some(footer.into()),
        false,
    )
}
