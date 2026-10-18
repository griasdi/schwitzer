//! The task list: foldable groups of raised slabs, each with its stage lamp, labels, the
//! worker's name tag and its next-step button.

use iced::widget::{Space, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};
use schwitzer_proto::{Decision, RoomState, Stage, Task};

use super::icons::{Icon, Tone, icon};
use super::logic::{self, Group};
use super::widgets::{
    self, icon_button, icon_text_button, id_button, lamp, member_tag, muted, priority_pill,
    with_tip,
};
use super::{App, Message};
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, SP_4, TS_12, TS_14};

/// Width of the stage lamp; the second line of a slab starts under the title.
const LAMP: f32 = 10.0;

pub fn view<'a>(app: &'a App, state: &'a RoomState) -> Element<'a, Message> {
    let mut list = column![].spacing(SP_3).width(Length::Fill);
    for group in Group::ALL {
        let tasks = logic::tasks_in(state, group);
        // Proposals and ideas stay visible, so their `+` is always reachable.
        if tasks.is_empty() && !matches!(group, Group::Proposals | Group::Ideas) {
            continue;
        }
        let open = !app.collapsed.contains(&group);
        let mut slabs = column![group_header(group, tasks.len(), open)].spacing(SP_1);
        if open {
            for task in tasks {
                slabs = slabs.push(task_row(app, state, task));
            }
        }
        list = list.push(slabs);
    }
    if state.tasks.is_empty() {
        list = list.push(
            container(muted(
                "No tasks yet. Add a proposal with +; agents pick it up once it has a plan.",
            ))
            .padding([SP_1, SP_2]),
        );
    }
    scrollable(container(list).padding(SP_2))
        .spacing(SP_1)
        .id("tasks")
        .height(Length::Fill)
        .style(theme::scroll)
        .into()
}

/// A centred empty state with one action.
pub fn empty<'a>(
    glyph: Icon,
    title: &'a str,
    hint: &'a str,
    action: Element<'a, Message>,
) -> Element<'a, Message> {
    container(
        column![
            icon(glyph, 40.0, Tone::Muted),
            text(title).size(TS_14).font(SEMIBOLD),
            muted(hint).align_x(Alignment::Center),
            action,
        ]
        .spacing(SP_2)
        .align_x(Alignment::Center)
        .max_width(280),
    )
    .center(Length::Fill)
    .padding(SP_4)
    .into()
}

fn group_header<'a>(group: Group, count: usize, open: bool) -> Element<'a, Message> {
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    let mut line = row![
        icon(chevron, 14.0, Tone::Muted),
        text(group.title()).size(TS_12).font(SEMIBOLD),
        widgets::pill(count.to_string()),
        Space::new().width(Length::Fill),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    // The `+` takes the press before the header does.
    match group {
        Group::Proposals => {
            line = line.push(icon_button(
                "add-proposal",
                Icon::Plus,
                Tone::Text,
                "New proposal  (Ctrl+N)",
                Some(Message::OpenNewTask { idea: false }),
                theme::button_ghost,
            ))
        }
        Group::Ideas => {
            line = line.push(icon_button(
                "add-idea",
                Icon::Plus,
                Tone::Text,
                "New idea",
                Some(Message::OpenNewTask { idea: true }),
                theme::button_ghost,
            ))
        }
        _ => {}
    }
    id_button(
        format!("group-{}", group.slug()),
        container(line)
            .padding([0.0, SP_1])
            .height(32)
            .align_y(Alignment::Center),
        Some(Message::ToggleGroup(group)),
        theme::button_header,
        Length::Fill,
    )
}

fn task_row<'a>(app: &'a App, state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let n = task.number;
    let mut meta = row![
        text(format!("#{n}"))
            .size(TS_12)
            .font(MEDIUM)
            .style(theme::text_muted),
        widgets::pill(logic::stage_label(task)),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    if let Some(priority) = priority_pill(task.priority) {
        meta = meta.push(priority);
    }
    meta = meta.push(Space::new().width(Length::Fill));
    if let Some(claim) = &task.claim {
        meta = meta.push(member_tag(
            logic::member_name(&state.members, &claim.member),
            logic::member_hue(&claim.member),
        ));
    }
    let mut line = row![
        column![
            row![
                lamp(task),
                text(&task.title)
                    .size(TS_14)
                    .font(MEDIUM)
                    .width(Length::Fill),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
            container(meta).padding(Padding {
                left: LAMP + SP_2,
                ..Padding::ZERO
            }),
        ]
        .spacing(SP_1)
        .width(Length::Fill),
    ]
    .spacing(SP_3)
    .align_y(Alignment::Center);
    if let Some(action) = next_step(app, task) {
        line = line.push(action);
    }
    id_button(
        format!("task-{n}"),
        container(line).padding([SP_2, SP_3]),
        Some(Message::OpenTask(n)),
        theme::slab,
        Length::Fill,
    )
}

/// The one button a slab carries: Pick a finished plan, Accept a proposal, or Release your own
/// claim. The button takes the press before the slab does.
fn next_step<'a>(app: &'a App, task: &'a Task) -> Option<Element<'a, Message>> {
    let n = task.number;
    if logic::pickable(task) {
        let busy = app.picking == Some(n);
        return Some(with_tip(
            icon_text_button(
                format!("pick-{n}"),
                Icon::Pick,
                if busy { Tone::Muted } else { Tone::OnPrimary },
                "Pick",
                (!busy && app.picking.is_none()).then_some(Message::Pick(n)),
                theme::button_primary,
            ),
            "Claim it and copy a prompt for your agent",
        ));
    }
    if task.stage == Stage::Proposed {
        return Some(icon_text_button(
            format!("accept-{n}"),
            Icon::Check,
            Tone::OnPrimary,
            "Accept",
            Some(Message::Decide(n, Decision::Accept)),
            theme::button_primary,
        ));
    }
    match &task.claim {
        Some(claim) if claim.member == app.client.member() => Some(with_tip(
            icon_text_button(
                format!("release-{n}"),
                Icon::Leave,
                Tone::Text,
                "Release",
                Some(Message::Release(n)),
                theme::button_secondary,
            ),
            "Give the claim back; someone else can take it",
        )),
        _ => None,
    }
}
