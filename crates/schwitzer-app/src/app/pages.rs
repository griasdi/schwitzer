//! Full-window pages with a back button: Settings and its topic pages, About.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::Message;
use super::icons::{Icon, Tone};
use super::widgets::{bar, bar_icon_button, hrule};
use crate::theme::{self, SEMIBOLD, SP_1, SP_2, SP_3, TS_20};

/// The widest the body of a page grows in a wide window.
const MAX_WIDTH: f32 = 560.0;

/// The widget id of a page's scrolling body. The pages share one, and iced keeps its offset
/// from page to page, so a page that should not open where the last one was scrolled snaps it.
pub const BODY_ID: &str = "page-body";

/// A page: header bar with back button and title, scrolling body, optional footer bar.
pub fn frame<'a>(
    id: &'static str,
    title: impl text::IntoFragment<'a>,
    body: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let header = bar(
        row![
            bar_icon_button(
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
        SP_2,
    );
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
        .style(theme::scroll)
        .id(BODY_ID),
    ];
    if let Some(footer) = footer {
        col = col.push(hrule()).push(bar(footer, [SP_2, SP_3]));
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
