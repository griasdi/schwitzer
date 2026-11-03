//! Task text set for reading: the Markdown viewer of the open task and the full-window reading
//! page.

use iced::widget::markdown as md;
use iced::widget::text::LineHeight;
use iced::widget::{checkbox, column, container, rich_text, row, scrollable, text};
use iced::{Element, Length, alignment};
use schwitzer_proto::{RoomState, Stage, Task};

use super::widgets::hrule;
use super::{App, Message, Reading, list, pages};
use crate::theme::{self, READ_LINE_HEIGHT, SP_1, SP_2, SP_3};

/// Task text as Markdown, set for reading; a click on a link opens it.
pub fn markdown<'a>(app: &'a App, parsed: &'a super::ParsedMarkdown) -> Element<'a, Message> {
    md::view_with(parsed.items(), theme::task_markdown(&app.theme), &Reader).map(Message::OpenLink)
}

/// The proposal or the plan of the open task across the window, under the task's summary
/// line. No actions: decisions stay in the open task.
pub fn page<'a>(
    app: &'a App,
    state: &'a RoomState,
    task: &'a Task,
    part: Reading,
) -> Element<'a, Message> {
    let (title, parsed) = match part {
        Reading::Proposal if task.stage == Stage::Idea => ("Idea", &app.open_md.body),
        Reading::Proposal => ("Proposal", &app.open_md.body),
        Reading::Plan => ("Plan", &app.open_md.plan),
    };
    let body = pages::section(
        column![
            list::summary(state, task, None),
            hrule(),
            markdown(app, parsed)
        ]
        .spacing(SP_3),
    );
    pages::reading_frame("reading-page", title, body)
}

/// The line height of reading text.
fn line_height() -> LineHeight {
    LineHeight::Relative(READ_LINE_HEIGHT)
}

/// iced's Markdown look with the reading line height on paragraphs and list markers, and code
/// blocks on the well's tone that scroll sideways instead of wrapping.
struct Reader;

impl<'a> md::Viewer<'a, md::Uri> for Reader {
    fn on_link_click(url: md::Uri) -> md::Uri {
        url
    }

    fn paragraph(&self, settings: md::Settings, text: &md::Text) -> Element<'a, md::Uri> {
        rich_text(text.spans(settings.style))
            .size(settings.text_size)
            .line_height(line_height())
            .on_link_click(Self::on_link_click)
            .into()
    }

    fn code_block(
        &self,
        settings: md::Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [md::Text],
    ) -> Element<'a, md::Uri> {
        // Plain spans carry the body face; in a code block they take the code face.
        let code = md::Style {
            font: settings.style.code_block_font,
            ..settings.style
        };
        container(
            scrollable(
                container(column(lines.iter().map(|line| {
                    rich_text(line.spans(code))
                        .on_link_click(Self::on_link_click)
                        .font(settings.style.code_block_font)
                        .size(settings.code_size)
                        .into()
                })))
                .padding([SP_2, SP_3]),
            )
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new()
                    .width(SP_1)
                    .scroller_width(SP_1),
            ))
            .style(theme::scroll),
        )
        .width(Length::Fill)
        .style(theme::code_block)
        .into()
    }

    fn unordered_list(
        &self,
        settings: md::Settings,
        bullets: &'a [md::Bullet],
    ) -> Element<'a, md::Uri> {
        column(bullets.iter().map(|bullet| {
            let (marker, items): (Element<'a, md::Uri>, _) = match bullet {
                md::Bullet::Point { items } => (
                    text("•")
                        .size(settings.text_size)
                        .line_height(line_height())
                        .into(),
                    items,
                ),
                md::Bullet::Task { items, done } => (
                    container(
                        checkbox(*done)
                            .size(settings.text_size)
                            .style(theme::task_check),
                    )
                    .center_y(line_height().to_absolute(settings.text_size))
                    .into(),
                    items,
                ),
            };
            row![marker, nested(self, settings, items)]
                .spacing(settings.spacing)
                .into()
        }))
        .spacing(settings.spacing * 0.75)
        .padding([0.0, settings.spacing.0])
        .into()
    }

    fn ordered_list(
        &self,
        settings: md::Settings,
        start: u64,
        bullets: &'a [md::Bullet],
    ) -> Element<'a, md::Uri> {
        let digits = ((start + bullets.len() as u64).max(1) as f32)
            .log10()
            .ceil();
        column(bullets.iter().enumerate().map(|(i, bullet)| {
            let (md::Bullet::Point { items } | md::Bullet::Task { items, .. }) = bullet;
            row![
                text!("{}.", i as u64 + start)
                    .size(settings.text_size)
                    .line_height(line_height())
                    .align_x(alignment::Horizontal::Right)
                    .width(settings.text_size * ((digits / 2.0).ceil() + 1.0)),
                nested(self, settings, items),
            ]
            .spacing(settings.spacing)
            .into()
        }))
        .spacing(settings.spacing * 0.75)
        .into()
    }
}

/// The items of a list entry, a little tighter than the list around them.
fn nested<'a>(
    reader: &Reader,
    settings: md::Settings,
    items: &'a [md::Item],
) -> Element<'a, md::Uri> {
    md::view_with(
        items,
        md::Settings {
            spacing: settings.spacing * 0.6,
            ..settings
        },
        reader,
    )
}
