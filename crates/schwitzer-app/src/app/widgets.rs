//! Small building blocks shared by the views.
//!
//! iced 0.14 buttons carry no id, so a button that has to be addressed (by tests, by the
//! headless control) sits in a container that has one.

use iced::widget::{Space, button, container, row, text, tooltip};
use iced::{Alignment, Element, Length, Theme};
use schwitzer_proto::{Stage, Task};

use super::Message;
use super::icons::{Bloom, Icon, Tone, icon};
use super::logic;
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, TS_12, TS_14};

/// A button with a widget id. `on_press: None` shows it disabled. The button itself has no
/// padding: the content brings its own.
pub fn id_button<'a>(
    id: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
    on_press: Option<Message>,
    style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
    width: Length,
) -> Element<'a, Message> {
    container(
        button(content)
            .on_press_maybe(on_press)
            .padding(0)
            .style(style)
            .width(width),
    )
    .id(id.into())
    .width(width)
    .into()
}

/// A text button: 32 px tall, label in medium weight.
pub fn text_button<'a>(
    id: impl Into<String>,
    label: &'a str,
    on_press: Option<Message>,
    style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Element<'a, Message> {
    id_button(
        id,
        container(text(label).size(TS_14).font(MEDIUM)).padding([6.0, SP_3]),
        on_press,
        style,
        Length::Shrink,
    )
}

/// A text button with a leading icon.
pub fn icon_text_button<'a>(
    id: impl Into<String>,
    glyph: Icon,
    tone: Tone,
    label: &'a str,
    on_press: Option<Message>,
    style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Element<'a, Message> {
    id_button(
        id,
        container(
            row![
                icon(glyph, 16.0, tone),
                text(label).size(TS_14).font(MEDIUM)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([6.0, SP_3]),
        on_press,
        style,
        Length::Shrink,
    )
}

/// An icon-only button with a tooltip; 32x32 with a 16 px glyph.
pub fn icon_button<'a>(
    id: impl Into<String>,
    glyph: Icon,
    tone: Tone,
    tip: &'a str,
    on_press: Option<Message>,
    style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Element<'a, Message> {
    let tone = if on_press.is_some() {
        tone
    } else {
        Tone::Muted
    };
    let button = id_button(
        id,
        container(icon(glyph, 16.0, tone)).padding(SP_2),
        on_press,
        style,
        Length::Shrink,
    );
    with_tip(button, tip)
}

/// `content` with a tooltip above it.
pub fn with_tip<'a>(
    content: impl Into<Element<'a, Message>>,
    tip: impl text::IntoFragment<'a>,
) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(tip).size(TS_12)).padding([SP_1, SP_2]),
        tooltip::Position::Top,
    )
    .gap(SP_1)
    .style(theme::tooltip)
    .into()
}

/// The flower colour of a task.
pub fn bloom(task: &Task) -> Bloom {
    match (task.stage, &task.claim) {
        (Stage::Proposed, _) => Bloom::New,
        (Stage::Ready, Some(_)) => Bloom::Running,
        (Stage::Done, _) => Bloom::Closed,
        (Stage::Rejected, _) => Bloom::Rejected,
        _ if logic::pickable(task) => Bloom::Pickable,
        _ => Bloom::Planning,
    }
}

/// The status flower of a task: outlined while only proposed, filled in its stage colour after.
pub fn flower<'a>(task: &Task, size: f32) -> Element<'a, Message> {
    let bloom = bloom(task);
    let glyph = if bloom == Bloom::New {
        Icon::FlowerOutline
    } else {
        Icon::Flower
    };
    icon(glyph, size, Tone::Bloom(bloom)).into()
}

/// A member's name tag in the member's colour.
pub fn member_tag<'a>(name: impl text::IntoFragment<'a>, hue: usize) -> Element<'a, Message> {
    container(text(name).size(TS_12).font(SEMIBOLD))
        .padding([2.0, SP_2])
        .style(theme::tag(hue))
        .into()
}

/// A quiet pill with a label.
pub fn pill<'a>(label: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    container(text(label).size(TS_12).font(MEDIUM))
        .padding([1.0, SP_2])
        .style(theme::pill)
        .into()
}

/// A small round status dot: `Some(true)` up, `Some(false)` down, `None` unknown.
pub fn dot<'a>(state: Option<bool>) -> Element<'a, Message> {
    container(Space::new().width(8).height(8))
        .style(theme::dot(state))
        .into()
}

/// A full-width one-pixel separator.
pub fn hrule<'a>() -> Element<'a, Message> {
    container(Space::new().height(1))
        .width(Length::Fill)
        .style(theme::rule)
        .into()
}

/// Secondary text.
pub fn muted<'a>(label: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(label).size(TS_12).style(theme::text_muted)
}

/// The heading of a block inside a dialog.
pub fn heading<'a>(label: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(label).size(TS_14).font(SEMIBOLD)
}

/// The dismissable error strip.
pub fn banner(message: &str) -> Element<'_, Message> {
    container(
        row![
            icon(Icon::Alert, 16.0, Tone::Danger),
            text(message).size(TS_12).width(Length::Fill),
            icon_button(
                "dismiss-banner",
                Icon::Close,
                Tone::Muted,
                "Dismiss",
                Some(Message::DismissBanner),
                theme::button_ghost,
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    )
    .padding([SP_1, SP_2])
    .width(Length::Fill)
    .style(theme::alert)
    .into()
}
