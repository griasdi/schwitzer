//! The buddy list of tasks: foldable groups, one row per task with its status flower, the
//! name tag of whoever works on it, and a Pick button on finished plans.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};
use schwitzer_proto::{Priority, RoomState, Task};

use super::icons::{Icon, Tone, icon};
use super::logic::{self, Group};
use super::widgets::{self, flower, id_button, member_tag, muted, with_tip};
use super::{App, Message};
use crate::theme::{self, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, SP_4, TS_12, TS_14};

pub fn view<'a>(app: &'a App, state: &'a RoomState) -> Element<'a, Message> {
    if state.tasks.is_empty() {
        return empty(
            "No tasks yet",
            "Propose the first one; agents pick it up once it has a plan.",
            widgets::icon_text_button(
                "empty-new-task",
                Icon::Plus,
                Tone::OnPrimary,
                "New task",
                Some(Message::OpenNewTask),
                theme::button_primary,
            ),
        );
    }
    let mut list = column![].spacing(2).width(Length::Fill);
    for group in Group::ALL {
        let tasks = logic::tasks_in(state, group);
        if tasks.is_empty() {
            continue;
        }
        let open = !app.collapsed.contains(&group);
        list = list.push(group_header(group, tasks.len(), open));
        if open {
            for task in tasks {
                list = list.push(task_row(app, state, task));
            }
        }
    }
    // The right inset keeps the Pick buttons clear of the scrollbar.
    scrollable(container(list).padding(Padding {
        top: SP_2,
        right: SP_3,
        bottom: SP_2,
        left: SP_2,
    }))
    .id("tasks")
    .height(Length::Fill)
    .style(theme::scroll)
    .into()
}

/// A centred empty state with one action.
pub fn empty<'a>(
    title: &'a str,
    hint: &'a str,
    action: Element<'a, Message>,
) -> Element<'a, Message> {
    container(
        column![
            icon(Icon::FlowerOutline, 40.0, Tone::Muted),
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
    id_button(
        format!("group-{}", group.slug()),
        container(
            row![
                icon(chevron, 14.0, Tone::Muted),
                text(group.title()).size(TS_12).font(SEMIBOLD),
                muted(count.to_string()),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([SP_1, SP_1]),
        Some(Message::ToggleGroup(group)),
        theme::button_row,
        Length::Fill,
    )
}

fn task_row<'a>(app: &'a App, state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let mut line = row![
        flower(task, 16.0),
        text(format!("#{}", task.number))
            .size(TS_12)
            .font(MEDIUM)
            .style(theme::text_muted),
        text(&task.title).size(TS_14).width(Length::Fill),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    match task.priority {
        Priority::High => {
            line = line.push(with_tip(
                icon(Icon::ArrowUp, 14.0, Tone::Danger),
                "High priority",
            ))
        }
        Priority::Low => {
            line = line.push(with_tip(
                icon(Icon::ArrowDown, 14.0, Tone::Muted),
                "Low priority",
            ))
        }
        Priority::Normal => {}
    }
    if let Some(claim) = &task.claim {
        line = line.push(muted(logic::work_label(claim.work)));
        line = line.push(member_tag(
            logic::member_name(&state.members, &claim.member),
            logic::member_hue(&claim.member),
        ));
    }
    if logic::pickable(task) {
        let busy = app.picking == Some(task.number);
        // Inside the row, so its right edge lines up with the tags of the other rows; the
        // button takes the press before the row does.
        line = line.push(with_tip(
            id_button(
                format!("pick-{}", task.number),
                container(
                    row![
                        icon(
                            Icon::Pick,
                            14.0,
                            if busy { Tone::Muted } else { Tone::OnPrimary }
                        ),
                        text("Pick").size(TS_12).font(SEMIBOLD),
                    ]
                    .spacing(SP_1)
                    .align_y(Alignment::Center),
                )
                .padding([SP_1, 10.0]),
                (!busy && app.picking.is_none()).then_some(Message::Pick(task.number)),
                theme::button_primary,
                Length::Shrink,
            ),
            "Claim it and copy a prompt for your agent",
        ));
    }
    id_button(
        format!("task-{}", task.number),
        container(line).padding([6.0, SP_2]),
        Some(Message::OpenTask(task.number)),
        theme::button_row,
        Length::Fill,
    )
}
