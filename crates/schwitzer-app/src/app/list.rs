//! The task list: foldable groups of raised slabs, each with its stage lamp, labels, the
//! worker's name tag and its next-step button; the open task or draft sits in place among them.

use iced::keyboard::{Key, key::Named};
use iced::widget::{Space, column, container, responsive, row, scrollable, text, text_editor};
use iced::{Alignment, Element, Length, Padding};
use schwitzer_proto::{Decision, Project, RoomState, Stage, Task};

use super::icons::{Icon, Tone, icon};
use super::logic::{self, Group, MissingSession};
use super::task_view;
use super::widgets::{
    self, LAMP, icon_button, icon_text_button, id_button, lamp, lamp_kind, member_tag_of, muted,
    priority_pill, with_tip,
};
use super::{App, Expanded, Message};
use crate::theme::{self, Lamp, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, SP_4, TS_12, TS_14};

/// The widget id of the task list's scrollable.
pub const LIST_ID: &str = "tasks";

pub fn view<'a>(app: &'a App, state: &'a RoomState, project: &'a Project) -> Element<'a, Message> {
    responsive(move |size| {
        let mut list = column![].spacing(SP_3).width(Length::Fill);
        for group in Group::ALL {
            let tasks = logic::tasks_in(state, project.id, group);
            // Proposals and ideas stay visible, so their `+` is always reachable.
            if tasks.is_empty() && !matches!(group, Group::Proposals | Group::Ideas) {
                continue;
            }
            let open = !app.collapsed.contains(&group);
            let mut slabs = column![group_header(group, tasks.len(), open)].spacing(SP_1);
            if open {
                if matches!(group, Group::Proposals | Group::Ideas)
                    && app.expanded
                        == Some(Expanded::New {
                            idea: group == Group::Ideas,
                        })
                {
                    slabs =
                        slabs.push(task_view::new_task(app, group == Group::Ideas, size.height));
                }
                for task in tasks {
                    slabs = slabs.push(if app.expanded == Some(Expanded::Task(task.number)) {
                        task_view::view(app, state, task, size.height)
                    } else {
                        task_row(app, state, task)
                    });
                }
            }
            list = list.push(slabs);
        }
        if !state.tasks.iter().any(|t| t.project == project.id) {
            list = list.push(
                container(muted(
                    "No tasks yet. Add a proposal with +; agents pick it up once it has a plan.",
                ))
                .padding([SP_1, SP_2]),
            );
        }
        scrollable(container(list).padding(SP_2))
            .spacing(SP_1)
            .id(LIST_ID)
            .height(Length::Fill)
            .style(theme::scroll)
            .into()
    })
    .into()
}

/// The quick idea bar between the task list and the sessions: Enter adds the text as an idea
/// (first line the title, the rest its details), Shift+Enter breaks the line.
pub fn idea_bar(app: &App) -> Element<'_, Message> {
    let editor = text_editor(&app.forms.quick_idea)
        .id("quick-idea")
        .placeholder("Note an idea, Enter adds it  (Ctrl+L)")
        .max_height(120.0)
        .padding(SP_2)
        .size(TS_14)
        .on_action(Message::QuickIdea)
        .key_binding(|kp| {
            if matches!(kp.status, text_editor::Status::Focused { .. })
                && matches!(kp.key.as_ref(), Key::Named(Named::Enter))
                && !kp.modifiers.shift()
            {
                Some(text_editor::Binding::Custom(Message::NoteIdea))
            } else {
                text_editor::Binding::from_key_press(kp)
            }
        })
        .style(theme::editor);
    // Level with the middle of the editor's first line.
    let lamp_box = container(lamp(Lamp::Idea)).padding(Padding {
        top: 12.0,
        ..Padding::ZERO
    });
    widgets::bar(
        row![lamp_box, editor]
            .spacing(SP_2)
            .align_y(Alignment::Start),
        SP_2,
    )
    .into()
}

/// A centred empty state with one action.
pub fn empty<'a>(
    glyph: Icon,
    title: impl text::IntoFragment<'a>,
    hint: impl text::IntoFragment<'a>,
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
    // The `+` takes the press before the header does.
    let add = match group {
        Group::Proposals => Some(icon_button(
            "add-proposal",
            Icon::Plus,
            Tone::Text,
            "New proposal  (Ctrl+N)",
            Some(Message::OpenNewTask { idea: false }),
            theme::button_ghost,
        )),
        Group::Ideas => Some(icon_button(
            "add-idea",
            Icon::Plus,
            Tone::Text,
            "New idea",
            Some(Message::OpenNewTask { idea: true }),
            theme::button_ghost,
        )),
        _ => None,
    };
    widgets::group_head(
        format!("group-{}", group.slug()),
        group.title(),
        count,
        open,
        Message::ToggleGroup(group),
        add,
    )
}

/// The two lines of a slab: lamp and title, then number, stage, priority, the tasks it still
/// waits for, `hint` (see [`session_hint`]) and the worker's tag.
pub fn summary<'a>(
    state: &'a RoomState,
    task: &'a Task,
    hint: Option<(&'static str, String)>,
) -> Element<'a, Message> {
    let mut meta = row![
        text(format!("#{}", task.number))
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
    let open = task.open_waits(&state.tasks);
    if logic::editable(task) && !open.is_empty() {
        meta = meta.push(muted(logic::waits_label(&open)));
    }
    meta = meta.push(Space::new().width(Length::Fill));
    if let Some((label, why)) = hint {
        meta = meta.push(with_tip(muted(label), why));
    }
    if let Some(claim) = &task.claim {
        meta = meta.push(member_tag_of(&state.members, &claim.member));
    }
    column![
        row![
            lamp(lamp_kind(task)),
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
    .width(Length::Fill)
    .into()
}

/// The hint a claim no omp session carries shows, and the sentence that explains it.
pub fn session_hint(app: &App, state: &RoomState, task: &Task) -> Option<(&'static str, String)> {
    let claim = task.claim.as_ref()?;
    let missing = logic::missing_session(
        state,
        task,
        app.client.member(),
        &app.local_sessions,
        app.now_ms,
    )?;
    let name = logic::member_name(&state.members, &claim.member);
    Some(match missing {
        MissingSession::Gone if claim.member == app.client.member() => (
            "no session",
            "None of your omp sessions carries this claim. If its agent stopped, Unclaim gives \
             the task back."
                .to_owned(),
        ),
        MissingSession::Gone => (
            "no session",
            format!("{name} shares no omp session that carries this claim."),
        ),
        MissingSession::Unknown => (
            "session unknown",
            format!("{name} is offline, so their omp sessions are not shared."),
        ),
    })
}

fn task_row<'a>(app: &'a App, state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let n = task.number;
    let mut line = row![summary(state, task, session_hint(app, state, task))]
        .spacing(SP_3)
        .align_y(Alignment::Center);
    if let Some(action) = next_step(app, state, task) {
        line = line.push(action);
    }
    id_button(
        format!("task-{n}"),
        container(line).padding([SP_2, SP_3]),
        Some(Message::ToggleTask(n)),
        theme::slab,
        Length::Fill,
    )
}

/// The one button a slab carries: Approve a finished plan, Pick an approved one, Accept a
/// proposal, or Unclaim your own claim. The button takes the press before the slab does.
/// Right after an approval Approve stays, busy, while the planning session claims the build.
fn next_step<'a>(
    app: &'a App,
    state: &'a RoomState,
    task: &'a Task,
) -> Option<Element<'a, Message>> {
    let n = task.number;
    if logic::approvable(task) {
        return Some(with_tip(
            icon_text_button(
                format!("approve-{n}"),
                Icon::Check,
                Tone::OnPrimary,
                "Approve",
                Some(Message::Decide(n, Decision::Approve)),
                theme::button_primary,
            ),
            "Approve the plan; an omp session waiting on its review builds it",
        ));
    }
    if app.handing_off(state.id, task) {
        return Some(with_tip(
            icon_text_button(
                format!("approve-{n}"),
                Icon::Check,
                Tone::Muted,
                "Approve",
                None,
                theme::button_primary,
            ),
            "Approved; the omp session that wrote the plan starts the build",
        ));
    }
    if logic::pickable(task) {
        let open = task.open_waits(&state.tasks);
        if !open.is_empty() {
            return Some(with_tip(
                icon_text_button(
                    format!("pick-{n}"),
                    Icon::Pick,
                    Tone::Muted,
                    "Pick",
                    None,
                    theme::button_primary,
                ),
                format!("Not pickable yet: it {}", logic::waits_label(&open)),
            ));
        }
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
                format!("unclaim-{n}"),
                Icon::Leave,
                Tone::Text,
                "Unclaim",
                Some(Message::Unclaim(n)),
                theme::button_secondary,
            ),
            "Give the claim back; someone else can take it",
        )),
        _ => None,
    }
}
