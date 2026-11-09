//! The history of the open task's details or plan and the review of an agent's suggestion,
//! each a full-window page under the task's summary line. State and requests are
//! `revisions.rs`.

use iced::widget::text::LineHeight;
use iced::widget::{Column, Space, column, container, row, text};
use iced::{Alignment, Element, Length};
use schwitzer_proto::{MemberId, Part, RoomState, Stage, Task};

use super::icons::{Icon, Tone, icon};
use super::logic::{self, DiffLine};
use super::revisions::{LoadedSuggestion, RevisionPage};
use super::widgets::{self, chevron, heading, hrule, id_button, member_tag_of, muted, text_button};
use super::{App, Message, list, pages};
use crate::theme::{self, MEDIUM, READ_LINE_HEIGHT, SP_1, SP_2, SP_3, TS_14, TS_16};

pub fn page<'a>(app: &'a App, state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let (id, title, content) = match &app.revisions {
        Some(RevisionPage::History {
            part,
            versions,
            selected,
            diff,
            ..
        }) => {
            let title = match part {
                Part::Plan => "History of the plan",
                Part::Details if task.stage == Stage::Idea => "History of the idea",
                Part::Details => "History of the proposal",
            };
            let content = match versions {
                None => muted("Loading…").into(),
                Some(list) => history(app, state, task, list, *selected, diff),
            };
            ("history-page", title, content)
        }
        Some(RevisionPage::Suggestion {
            id, loaded, busy, ..
        }) => {
            let part = loaded.as_ref().map(|l| l.suggestion.part).or_else(|| {
                task.suggestions
                    .iter()
                    .find(|s| s.id == *id)
                    .map(|s| s.part)
            });
            let title = match part {
                Some(Part::Plan) => "Suggested plan",
                _ if task.stage == Stage::Idea => "Suggested idea",
                _ => "Suggested proposal",
            };
            (
                "suggestion-page",
                title,
                suggestion(app, state, task, *id, loaded.as_ref(), *busy),
            )
        }
        None => ("history-page", "History", Space::new().into()),
    };
    let lane = app.settings.fast_lane_task(&state.id, task.number);
    let body = pages::section(
        column![list::summary(state, task, None, lane), hrule(), content].spacing(SP_3),
    );
    pages::reading_frame(id, title, body)
}

/// Author, agent and age of a version or a suggestion; no author means the version from before
/// the history was kept.
fn who<'a>(
    app: &App,
    state: &'a RoomState,
    author: Option<&MemberId>,
    agent: Option<&'a str>,
    at_ms: u64,
) -> iced::widget::Row<'a, Message> {
    let mut line = row![].spacing(6).align_y(Alignment::Center);
    line = match author {
        Some(author) => line.push(member_tag_of(&state.members, author)),
        None => line.push(muted("before the history")),
    };
    if let Some(agent) = agent {
        line = line
            .push(icon(Icon::Bot, 14.0, Tone::Muted))
            .push(muted(agent));
    }
    line.push(Space::new().width(Length::Fill))
        .push(muted(format!(
            "{} ago",
            logic::ago(app.now_ms.saturating_sub(at_ms))
        )))
}

fn history<'a>(
    app: &'a App,
    state: &'a RoomState,
    task: &'a Task,
    versions: &'a [schwitzer_proto::Revision],
    selected: u64,
    diff: &'a [DiffLine],
) -> Element<'a, Message> {
    let newest = versions.first().map(|v| v.version);
    let mut col = column![].spacing(SP_2);
    for (i, v) in versions.iter().enumerate() {
        let open = v.version == selected;
        let mut head = row![
            chevron(open),
            text(format!("Version {}", v.version))
                .size(TS_14)
                .font(MEDIUM),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center);
        if Some(v.version) == newest {
            head = head.push(widgets::pill("current"));
        }
        head = head.push(who(
            app,
            state,
            v.author.as_ref(),
            v.agent.as_deref(),
            v.at_ms,
        ));
        col = col.push(id_button(
            format!("version-{}", v.version),
            container(head).padding([SP_1, SP_2]),
            Some(Message::SelectVersion(v.version)),
            theme::button_row,
            Length::Fill,
        ));
        if !open {
            continue;
        }
        let mut detail = column![diff_view(diff)].spacing(SP_2);
        if v.version > 1 && versions.get(i + 1).is_none() {
            detail = detail.push(muted("The version before is no longer kept."));
        }
        if Some(v.version) != newest && logic::editable(task) {
            detail = detail.push(row![
                Space::new().width(Length::Fill),
                text_button(
                    "restore-version",
                    "Restore",
                    Some(Message::RestoreVersion(v.version)),
                    theme::button_secondary,
                ),
            ]);
        }
        col = col.push(container(detail).padding([0.0, SP_2]));
    }
    if versions.last().is_some_and(|v| v.version > 1) {
        col = col.push(muted("Older versions are not shown."));
    }
    col.into()
}

fn suggestion<'a>(
    app: &'a App,
    state: &'a RoomState,
    task: &'a Task,
    id: u64,
    loaded: Option<&'a LoadedSuggestion>,
    busy: bool,
) -> Element<'a, Message> {
    let Some(l) = loaded else {
        return muted("Loading…").into();
    };
    let s = &l.suggestion;
    let mut col = column![who(
        app,
        state,
        Some(&s.author),
        s.agent.as_deref(),
        s.at_ms
    )]
    .spacing(SP_3);
    if !s.note.is_empty() {
        col = col.push(
            text(&s.note)
                .size(TS_16)
                .line_height(LineHeight::Relative(READ_LINE_HEIGHT)),
        );
    }
    if logic::outdated(task, s) {
        let part = match s.part {
            Part::Details => "details are",
            Part::Plan => "plan is",
        };
        col = col.push(muted(format!(
            "Written on version {base}; the {part} at version {current} now. Apply replaces \
             the current text, changes since version {base} included.",
            base = s.version,
            current = task.version(s.part),
        )));
    }
    col.push(heading("Changes against the current text"))
        .push(diff_view(&l.diff))
        .push(
            row![
                Space::new().width(Length::Fill),
                text_button(
                    "discard-suggestion",
                    "Discard",
                    (!busy).then_some(Message::DiscardSuggestion(id)),
                    theme::button_danger,
                ),
                text_button(
                    "apply-suggestion",
                    "Apply",
                    (!busy && logic::editable(task)).then_some(Message::ApplySuggestion(id)),
                    theme::button_primary,
                ),
            ]
            .spacing(SP_2),
        )
        .into()
}

/// The lines of a diff: added ones on the Leaf tint with `+`, removed ones on the danger tint
/// with `−`, a gap of left-out lines as `…`.
fn diff_view(lines: &[DiffLine]) -> Element<'_, Message> {
    if lines.is_empty() {
        return muted("No change.").into();
    }
    Column::with_children(lines.iter().map(|l| {
        match l {
            DiffLine::Added(s) => diff_row("+", theme::text_ink, s)
                .style(theme::diff_added)
                .into(),
            DiffLine::Removed(s) => diff_row("−", theme::text_danger, s)
                .style(theme::diff_removed)
                .into(),
            DiffLine::Same(s) => diff_row(" ", theme::text_muted, s).into(),
            DiffLine::Gap => container(muted("…")).padding([0.0, SP_1]).into(),
        }
    }))
    .into()
}

/// One line of a diff behind its marker.
fn diff_row<'a>(
    marker: &'static str,
    marker_style: fn(&iced::Theme) -> text::Style,
    body: &'a str,
) -> iced::widget::Container<'a, Message> {
    container(
        row![
            text(marker)
                .size(TS_14)
                .font(MEDIUM)
                .style(marker_style)
                .width(Length::Fixed(12.0)),
            text(body).size(TS_14).width(Length::Fill),
        ]
        .spacing(SP_1),
    )
    .padding([0.0, SP_1])
    .width(Length::Fill)
}
