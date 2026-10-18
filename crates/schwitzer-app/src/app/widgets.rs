//! Small building blocks shared by the views.
//!
//! iced 0.14 buttons carry no id, so a button that has to be addressed (by tests, by the
//! headless control) sits in a container that has one.

use iced::widget::{Space, button, container, row, text, text_input, tooltip};
use iced::{Alignment, Element, Length, Theme};
use schwitzer_proto::{Priority, Stage, Task};

use super::Message;
use super::icons::{Icon, Tone, icon};
use super::logic;
use crate::theme::{self, Lamp, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, TS_12, TS_14};

/// A text input with a widget id that submits with Enter.
pub fn input<'a>(
    id: &'static str,
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Message,
    on_submit: Message,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .id(id)
        .on_input(on_input)
        .on_submit(on_submit)
        .padding(SP_2)
        .size(TS_14)
        .style(theme::input)
}

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
    with_tip(icon_only(id, glyph, tone, on_press, style), tip)
}

/// An icon button in a bar at the top of the window: like [`icon_button`], but its
/// tooltip opens below it, because above it there is no room and iced would push the
/// tip over the button.
pub fn bar_icon_button<'a>(
    id: impl Into<String>,
    glyph: Icon,
    tone: Tone,
    tip: &'a str,
    on_press: Option<Message>,
    style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Element<'a, Message> {
    bar_tip(icon_only(id, glyph, tone, on_press, style), tip)
}

/// `content` with a tooltip below it: for controls in a bar at the top of the window.
pub fn bar_tip<'a>(
    content: impl Into<Element<'a, Message>>,
    tip: impl text::IntoFragment<'a>,
) -> Element<'a, Message> {
    tip_at(content, tip, tooltip::Position::Bottom)
}

/// The 32x32 icon button without a tooltip; its glyph is muted while it is disabled.
fn icon_only<'a>(
    id: impl Into<String>,
    glyph: Icon,
    tone: Tone,
    on_press: Option<Message>,
    style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Element<'a, Message> {
    let tone = if on_press.is_some() {
        tone
    } else {
        Tone::Muted
    };
    id_button(
        id,
        container(icon(glyph, 16.0, tone)).padding(SP_2),
        on_press,
        style,
        Length::Shrink,
    )
}

/// `content` with a tooltip above it.
pub fn with_tip<'a>(
    content: impl Into<Element<'a, Message>>,
    tip: impl text::IntoFragment<'a>,
) -> Element<'a, Message> {
    tip_at(content, tip, tooltip::Position::Top)
}

/// `content` with a tooltip at `position`, 4 px away from it.
fn tip_at<'a>(
    content: impl Into<Element<'a, Message>>,
    tip: impl text::IntoFragment<'a>,
    position: tooltip::Position,
) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(tip).size(TS_12)).padding([SP_1, SP_2]),
        position,
    )
    .gap(SP_1)
    .style(theme::tooltip)
    .into()
}

/// The lamp kind of a task.
pub fn lamp_kind(task: &Task) -> Lamp {
    match (task.stage, &task.claim) {
        (Stage::Idea, _) => Lamp::Idea,
        (Stage::Proposed, _) => Lamp::New,
        (Stage::Accepted | Stage::Ready, Some(_)) => Lamp::Running,
        (Stage::Done, _) => Lamp::Closed,
        (Stage::Rejected, _) => Lamp::Rejected,
        _ if logic::pickable(task) => Lamp::Pickable,
        _ => Lamp::Planning,
    }
}

/// Width of the stage lamp; the second line of a slab starts under the title.
pub const LAMP: f32 = 10.0;

/// A stage lamp: a 10 px square, hollow while an idea or only proposed.
pub fn lamp<'a>(kind: Lamp) -> Element<'a, Message> {
    container(Space::new().width(LAMP).height(LAMP))
        .style(theme::lamp(kind))
        .into()
}

/// The priority of a task as a pill; nothing for normal priority.
pub fn priority_pill<'a>(priority: Priority) -> Option<Element<'a, Message>> {
    let (glyph, tone, label, style): (_, _, _, fn(&Theme) -> text::Style) = match priority {
        Priority::High => (Icon::ArrowUp, Tone::Danger, "High", theme::text_danger),
        Priority::Low => (Icon::ArrowDown, Tone::Muted, "Low", theme::text_muted),
        Priority::Normal => return None,
    };
    Some(
        container(
            row![
                icon(glyph, 12.0, tone),
                text(label).size(TS_12).font(MEDIUM).style(style)
            ]
            .spacing(SP_1)
            .align_y(Alignment::Center),
        )
        .padding([1.0, SP_2])
        .style(theme::pill)
        .into(),
    )
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

/// A path in body text: fills its row and wraps after its separators, inside a name only
/// when the name alone is wider than the line.
pub fn path_text<'a>(path: &str) -> iced::widget::Text<'a> {
    text(logic::breakable_path(path))
        .size(TS_14)
        .width(Length::Fill)
        .wrapping(text::Wrapping::WordOrGlyph)
}

/// The heading of a block in a page section or in the open task.
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
