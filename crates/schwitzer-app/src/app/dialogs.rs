//! The modal layer and the frame of the task dialog.

use iced::widget::{column, container, mouse_area, opaque, row, scrollable, stack};
use iced::{Alignment, Element, Length, Padding};

use super::Message;
use super::icons::{Icon, Tone};
use super::widgets::icon_button;
use crate::theme::{self, SP_2, SP_3, SP_4};

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
            .on_press(Message::Back)
        )
    ]
    .into()
}

/// A dialog card: `header` with a close button, `body` scrolling below, `footer` pinned under
/// it. The card is as tall as its content, up to the window.
pub fn frame<'a>(
    id: &'static str,
    header: Element<'a, Message>,
    body: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let top = row![
        container(header).width(Length::Fill),
        icon_button(
            "close-dialog",
            Icon::Close,
            Tone::Muted,
            "Close  (Esc)",
            Some(Message::Back),
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
        .height(Length::Shrink)
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
        .style(theme::dialog)
        .into()
}
