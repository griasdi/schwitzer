//! Task text set for reading: the Markdown viewer of the open task and the full-window reading
//! page.

use iced::widget::markdown as md;
use iced::widget::{
    Space, checkbox, column, container, hover, rich_text, row, scrollable, text, text_editor,
};
use iced::{Element, Length, alignment};
use schwitzer_proto::{Annotation, Part, RoomState, Stage, Task};

use super::annotations::AnnotationDraft;
use super::icons::{Icon, icon};
use super::widgets::{byline, group_head, hrule, id_button, member_tag_of, text_button, with_tip};
use super::{App, Message, Reading, list, logic, pages};
use crate::theme::{self, SEMIBOLD, SP_1, SP_2, SP_3, TS_12, TS_14, Tone};

/// Task text as Markdown, set for reading; a click on a link opens it.
pub fn markdown<'a>(app: &'a App, parsed: &'a super::ParsedMarkdown) -> Element<'a, Message> {
    md::view_with(
        parsed.items(),
        theme::task_markdown(&app.theme),
        &Reader::PLAIN,
    )
    .map(Message::OpenLink)
}

/// `part` of the open task as Markdown, block by block, with the comments on each block
/// under it. While the task is open for comments, hovering a block shows the button that
/// comments on it. Comments whose block is gone follow under "Outdated comments"; resolved
/// ones fold under "Resolved comments", each with the block it quoted.
pub fn annotated<'a>(
    app: &'a App,
    state: &'a RoomState,
    task: &'a Task,
    part: Part,
) -> Element<'a, Message> {
    let settings = theme::task_markdown(&app.theme);
    let blocks = &app.open_md.part(part).blocks;
    let shown: Vec<&Annotation> = task
        .annotations
        .iter()
        .filter(|a| a.part == part && !a.resolved)
        .collect();
    let draft = app.annotating.as_ref().filter(|d| d.part == part);
    let editable = task.is_open();
    let mut placed = vec![false; shown.len()];
    let mut draft_placed = false;

    let mut outer = column![].spacing(settings.spacing);
    // The blocks of the top-level list being collected, and which list it is.
    let mut list: Option<(usize, iced::widget::Column<'a, Message>)> = None;
    for (i, block) in blocks.iter().enumerate() {
        let items = block.content.items();
        if items.is_empty() {
            continue;
        }
        let reader = Reader {
            ordered: block.list.and_then(|l| Some((l.number?, l.digits))),
        };
        let view: Element<'a, md::Uri> = column(
            items
                .iter()
                .map(|item| md::item(&reader, settings, item, i)),
        )
        .spacing(settings.spacing)
        .into();
        let view = view.map(Message::OpenLink);
        let view: Element<'a, Message> = if editable {
            let framed = row![container(view).width(Length::Fill), Space::new().width(32)];
            if app.annotating.is_none() {
                // 24 px, so it keeps its 16 px glyph beside a single line of text.
                let button = id_button(
                    format!("comment-{}-{i}", logic::part_name(part)),
                    container(icon(Icon::CommentAdd, 16.0, Tone::Muted)).padding(SP_1),
                    Some(Message::Annotate(part, i)),
                    theme::button_ghost,
                    Length::Shrink,
                );
                hover(
                    framed,
                    container(with_tip(button, "Comment on this block")).align_right(Length::Fill),
                )
            } else {
                framed.into()
            }
        } else {
            view
        };
        let mut entry = column![view].spacing(SP_2);
        for (a, done) in shown.iter().zip(placed.iter_mut()) {
            if !*done && a.quote == block.quote {
                *done = true;
                entry = entry.push(comment_card(app, state, a));
            }
        }
        if let Some(d) = draft
            && !draft_placed
            && d.quote == block.quote
        {
            draft_placed = true;
            entry = entry.push(comment_editor(d));
        }

        let place = block.list.map(|l| l.list);
        if list.as_ref().map(|(n, _)| *n) != place
            && let Some((_, items)) = list.take()
        {
            outer = outer.push(items);
        }
        match place {
            Some(n) => {
                let items = list
                    .take()
                    .map_or_else(|| column![].spacing(settings.spacing * 0.75), |(_, c)| c);
                list = Some((n, items.push(entry)));
            }
            None => outer = outer.push(entry),
        }
    }
    if let Some((_, items)) = list.take() {
        outer = outer.push(items);
    }

    let well = |quote: &'a str| {
        container(text(quote).size(TS_12).style(theme::text_muted))
            .padding([SP_1, SP_2])
            .width(Length::Fill)
            .style(theme::code_block)
    };
    let unplaced: Vec<&Annotation> = shown
        .iter()
        .zip(&placed)
        .filter(|(_, done)| !**done)
        .map(|(a, _)| *a)
        .collect();
    let stray_draft = draft.filter(|_| !draft_placed);
    if !unplaced.is_empty() || stray_draft.is_some() {
        let mut outdated = column![
            text("Outdated comments")
                .size(TS_12)
                .font(SEMIBOLD)
                .style(theme::text_muted)
        ]
        .spacing(SP_2);
        for a in unplaced {
            outdated = outdated.push(well(&a.quote));
            outdated = outdated.push(comment_card(app, state, a));
        }
        if let Some(d) = stray_draft {
            outdated = outdated.push(well(&d.quote));
            outdated = outdated.push(comment_editor(d));
        }
        outer = outer.push(outdated);
    }

    let resolved: Vec<&Annotation> = task
        .annotations
        .iter()
        .filter(|a| a.part == part && a.resolved)
        .collect();
    if !resolved.is_empty() {
        let mut fold = column![group_head(
            format!("toggle-resolved-{}", logic::part_name(part)),
            "Resolved comments",
            resolved.len(),
            app.show_resolved,
            Message::ToggleResolved,
            None,
        )]
        .spacing(SP_2);
        if app.show_resolved {
            for a in resolved {
                fold = fold.push(well(&a.quote));
                fold = fold.push(comment_card(app, state, a));
            }
        }
        outer = outer.push(fold);
    }
    outer.into()
}

/// A comment under its block: author, age, Resolve or Reopen, and the text.
fn comment_card<'a>(app: &'a App, state: &'a RoomState, a: &'a Annotation) -> Element<'a, Message> {
    let head = byline(
        member_tag_of(&state.members, &a.author),
        None,
        logic::age(app.now_ms, a.at_ms),
    )
    .push(text_button(
        format!("resolve-comment-{}", a.id),
        if a.resolved { "Reopen" } else { "Resolve" },
        Some(Message::ResolveAnnotation(a.id, !a.resolved)),
        theme::button_secondary,
    ));
    let body = text(&a.text).size(TS_14);
    let body = if a.resolved {
        body.style(theme::text_muted)
    } else {
        body
    };
    container(column![head, body].spacing(SP_1))
        .padding([SP_1, SP_2])
        .width(Length::Fill)
        .style(theme::comment)
        .into()
}

/// The editor of a new comment, with Cancel and Comment.
fn comment_editor(draft: &AnnotationDraft) -> Element<'_, Message> {
    let can_send = !draft.sending && !draft.text.text().trim().is_empty();
    column![
        text_editor(&draft.text)
            .id("comment-editor")
            .placeholder("Comment on this block")
            .min_height(64)
            .padding(SP_2)
            .size(TS_14)
            .on_action(Message::AnnotationAction)
            .style(theme::editor),
        row![
            Space::new().width(Length::Fill),
            text_button(
                "cancel-comment",
                "Cancel",
                Some(Message::CancelAnnotation),
                theme::button_secondary
            ),
            text_button(
                "save-comment",
                "Comment",
                can_send.then_some(Message::SaveAnnotation),
                theme::button_primary
            ),
        ]
        .spacing(SP_2),
    ]
    .spacing(SP_2)
    .into()
}

/// The proposal or the plan of the open task across the window, under the task's summary
/// line, with its comments. Decisions stay in the open task.
pub fn page<'a>(
    app: &'a App,
    state: &'a RoomState,
    task: &'a Task,
    part: Reading,
) -> Element<'a, Message> {
    let (title, part) = match part {
        Reading::Proposal if task.stage == Stage::Idea => ("Idea", Part::Details),
        Reading::Proposal => ("Proposal", Part::Details),
        Reading::Plan => ("Plan", Part::Plan),
    };
    let body = pages::section(
        column![
            list::summary(
                state,
                task,
                app.client.member(),
                None,
                app.settings.fast_lane_task(&state.id, task.number)
            ),
            hrule(),
            annotated(app, state, task, part)
        ]
        .spacing(SP_3),
    );
    pages::reading_frame("reading-page", title, body)
}

/// iced's Markdown look with the reading line height on paragraphs and list markers, and code
/// blocks on the well's tone that scroll sideways instead of wrapping.
///
/// `ordered` is the number and digit width of a block that is one item of a larger ordered list,
/// so it keeps the list's numbering and number column.
struct Reader {
    ordered: Option<(u64, f32)>,
}

impl Reader {
    /// A reader that numbers every list itself.
    const PLAIN: Reader = Reader { ordered: None };
}

impl<'a> md::Viewer<'a, md::Uri> for Reader {
    fn on_link_click(url: md::Uri) -> md::Uri {
        url
    }

    fn paragraph(&self, settings: md::Settings, text: &md::Text) -> Element<'a, md::Uri> {
        rich_text(text.spans(settings.style))
            .size(settings.text_size)
            .line_height(theme::read_line_height())
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
                        .line_height(theme::read_line_height())
                        .into(),
                    items,
                ),
                md::Bullet::Task { items, done } => (
                    container(
                        checkbox(*done)
                            .size(settings.text_size)
                            .style(theme::task_check),
                    )
                    .center_y(theme::read_line_height().to_absolute(settings.text_size))
                    .into(),
                    items,
                ),
            };
            row![marker, nested(settings, items)]
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
        let (first, digits) = self.ordered.unwrap_or_else(|| {
            let digits = ((start + bullets.len() as u64).max(1) as f32)
                .log10()
                .ceil();
            (start, digits)
        });
        column(bullets.iter().enumerate().map(|(i, bullet)| {
            let (md::Bullet::Point { items } | md::Bullet::Task { items, .. }) = bullet;
            row![
                text!("{}.", i as u64 + first)
                    .size(settings.text_size)
                    .line_height(theme::read_line_height())
                    .align_x(alignment::Horizontal::Right)
                    .width(settings.text_size * ((digits / 2.0).ceil() + 1.0)),
                nested(settings, items),
            ]
            .spacing(settings.spacing)
            .into()
        }))
        .spacing(settings.spacing * 0.75)
        .into()
    }
}

/// The items of a list entry, a little tighter than the list around them.
fn nested(settings: md::Settings, items: &[md::Item]) -> Element<'_, md::Uri> {
    md::view_with(
        items,
        md::Settings {
            spacing: settings.spacing * 0.6,
            ..settings
        },
        &Reader::PLAIN,
    )
}
