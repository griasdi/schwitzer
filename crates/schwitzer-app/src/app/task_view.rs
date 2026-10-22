//! The element open in place in the task list: a task shown in full, or the draft of a new one.

use iced::widget::{
    Space, column, container, markdown, pick_list, row, text, text_editor, text_input,
};
use iced::{Alignment, Element, Length, Padding};
use schwitzer_proto::{Decision, Priority, RoomId, RoomState, Stage, Task};

use super::chrome::ProjectChoice;
use super::icons::{Icon, Tone, icon};
use super::list;
use super::logic;
use super::widgets::{
    self, LAMP, heading, hrule, icon_button, icon_text_button, id_button, lamp, member_tag, muted,
    text_button,
};
use super::{App, Message, TaskText, sticky};
use crate::theme::{self, Lamp, MEDIUM, SEMIBOLD, SP_1, SP_2, SP_3, TS_12, TS_14};

/// Task `task` open in place: the slab's two lines as a head that closes it again, then
/// details, people, decisions, plan, priority and notes.
pub fn view<'a>(
    app: &'a App,
    state: &'a RoomState,
    task: &'a Task,
    list_height: f32,
) -> Element<'a, Message> {
    let n = task.number;
    let editable = logic::editable(task);
    let mut icons = row![];
    if app.task_draft.is_none() {
        icons = icons.push(icon_button(
            "copy-details",
            Icon::Copy,
            Tone::Muted,
            "Copy title and details as Markdown",
            Some(Message::CopyText(TaskText::Details)),
            theme::button_ghost,
        ));
    }
    if editable && app.task_draft.is_none() {
        icons = icons.push(icon_button(
            "edit-task",
            Icon::Pencil,
            Tone::Muted,
            "Edit title and details",
            Some(Message::EditTask),
            theme::button_ghost,
        ));
    }
    icons = icons.push(icon_button(
        "close-task",
        Icon::Close,
        Tone::Muted,
        "Close  (Esc)",
        Some(Message::Back),
        theme::button_ghost,
    ));
    let head = id_button(
        format!("task-{n}"),
        container(
            row![list::summary(state, task), icons]
                .spacing(SP_3)
                .align_y(Alignment::Center),
        )
        .padding([SP_2, SP_3]),
        Some(Message::ToggleTask(n)),
        theme::button_row,
        Length::Fill,
    );

    let mut body = column![].spacing(SP_3);
    if let Some(details) = details(app, task, list_height) {
        body = body.push(details);
    }
    body = body.push(people(app, state, task));
    if let Some(actions) = actions(app, state.id, task) {
        body = body.push(actions);
    }
    body = body
        .push(hrule())
        .push(plan(app, task, editable, list_height))
        .push(hrule())
        .push(priority(task))
        .push(hrule());
    if state.projects.len() > 1 {
        body = body.push(project(state, task)).push(hrule());
    }
    body = body.push(notes(app, state, task));
    container(sticky::sticky(
        column![head, hrule()],
        container(body).padding(SP_3),
        theme::pinned_head,
    ))
    .id("open-task")
    .width(Length::Fill)
    .style(theme::slab_open)
    .into()
}

/// What the open task, its form and their paddings take beside the details editor.
const DETAILS_RESERVE: f32 = 200.0;
/// What a draft's group header, title field, buttons and paddings take beside its editor.
const DRAFT_RESERVE: f32 = 170.0;
/// What the pinned head, the Plan heading, buttons and paddings take beside the plan editor.
const PLAN_RESERVE: f32 = 200.0;

/// The tallest an editor may grow in a list `list_height` tall: what `reserve` leaves, never
/// below `min`.
fn editor_max(list_height: f32, reserve: f32, min: f32) -> f32 {
    (list_height - reserve).max(min)
}

/// The draft of a new proposal, or of an idea, at the top of its group.
pub fn new_task(app: &App, idea: bool, list_height: f32) -> Element<'_, Message> {
    let filled = !app.forms.proposal_title.trim().is_empty();
    let (title_hint, body_hint, submit) = if idea {
        (
            "What's the idea?",
            "A few more words (optional)",
            "Add idea",
        )
    } else {
        (
            "What should be done?",
            "Details, context, links (optional)",
            "Propose",
        )
    };
    container(
        column![
            row![
                lamp(if idea { Lamp::Idea } else { Lamp::New }),
                text_input(title_hint, &app.forms.proposal_title)
                    .id("proposal-title")
                    .on_input(Message::ProposalTitle)
                    .on_submit(Message::Propose)
                    .padding(SP_2)
                    .size(TS_14)
                    .style(theme::input),
            ]
            .spacing(SP_2)
            .align_y(Alignment::Center),
            container(
                text_editor(&app.forms.proposal_body)
                    .id("proposal-body")
                    .placeholder(body_hint)
                    .min_height(100)
                    .max_height(editor_max(list_height, DRAFT_RESERVE, 100.0))
                    .padding(SP_2)
                    .size(TS_14)
                    .on_action(Message::ProposalBody)
                    .style(theme::editor),
            )
            .padding(Padding {
                left: LAMP + SP_2,
                ..Padding::ZERO
            }),
            row![
                Space::new().width(Length::Fill),
                text_button(
                    "cancel-new-task",
                    "Cancel",
                    Some(Message::Back),
                    theme::button_secondary
                ),
                text_button(
                    "propose",
                    submit,
                    filled.then_some(Message::Propose),
                    theme::button_primary
                ),
            ]
            .spacing(SP_2),
        ]
        .spacing(SP_2),
    )
    .padding([SP_2, SP_3])
    .id("new-task")
    .width(Length::Fill)
    .style(theme::slab_open)
    .into()
}

/// The task's details, or the form that edits its title and details.
fn details<'a>(app: &'a App, task: &'a Task, list_height: f32) -> Option<Element<'a, Message>> {
    if let Some(draft) = &app.task_draft {
        let idea = task.stage == Stage::Idea;
        let titled = !draft.title.trim().is_empty();
        let mut buttons = row![
            Space::new().width(Length::Fill),
            text_button(
                "cancel-task-edit",
                "Cancel",
                Some(Message::CancelEditTask),
                theme::button_secondary
            ),
            text_button(
                "save-task",
                "Save",
                titled.then_some(Message::SaveTask),
                if idea {
                    theme::button_secondary
                } else {
                    theme::button_primary
                },
            ),
        ]
        .spacing(SP_2);
        if idea {
            buttons = buttons.push(text_button(
                "propose-idea",
                "Propose",
                titled.then_some(Message::ProposeIdea),
                theme::button_primary,
            ));
        }
        return Some(
            column![
                text_input("Title", &draft.title)
                    .id("task-title")
                    .on_input(Message::EditTitle)
                    .on_submit(Message::SaveTask)
                    .padding(SP_2)
                    .size(TS_14)
                    .style(theme::input),
                text_editor(&draft.body)
                    .id("task-body")
                    .placeholder("Details")
                    .min_height(100)
                    .max_height(editor_max(list_height, DETAILS_RESERVE, 100.0))
                    .padding(SP_2)
                    .size(TS_14)
                    .on_action(Message::EditBody)
                    .style(theme::editor),
                buttons,
            ]
            .spacing(SP_2)
            .into(),
        );
    }
    (!task.body.is_empty()).then(|| rich(app, &app.open_md.body))
}

/// Task text as Markdown; a click on a link opens it.
fn rich<'a>(app: &'a App, md: &'a super::ParsedMarkdown) -> Element<'a, Message> {
    markdown::view(md.items(), theme::task_markdown(&app.theme)).map(Message::OpenLink)
}

/// Who proposed it and who works on it.
fn people<'a>(app: &'a App, state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let mut line = row![
        muted(if task.stage == Stage::Idea {
            "Idea by"
        } else {
            "Proposed by"
        }),
        member_tag(
            logic::member_name(&state.members, &task.author),
            logic::member_hue(&task.author)
        ),
        muted(format!(
            "{} ago",
            logic::ago(app.now_ms.saturating_sub(task.created_ms))
        )),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let Some(claim) = &task.claim else {
        return line.into();
    };
    line = line.push(Space::new().width(Length::Fill));
    column![
        line,
        row![
            icon(Icon::Bot, 14.0, Tone::Muted),
            member_tag(
                logic::member_name(&state.members, &claim.member),
                logic::member_hue(&claim.member)
            ),
            muted(format!(
                "{} {} for {}",
                claim.agent,
                logic::work_label(claim.work),
                logic::ago(app.now_ms.saturating_sub(claim.since_ms)),
            )),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(SP_2)
    .into()
}

/// Pick (when an approved plan waits) and the human decisions the server allows now. Right
/// after an approval only the busy "Approve & build" shows while the planning session claims
/// the build.
fn actions<'a>(app: &'a App, room: RoomId, task: &'a Task) -> Option<Element<'a, Message>> {
    let mut row = row![].spacing(SP_2).align_y(Alignment::Center);
    if app.handing_off(room, task) {
        return Some(
            row.push(text_button(
                "approve",
                "Approve & build",
                None,
                theme::button_primary,
            ))
            .into(),
        );
    }
    let mut any = false;
    let pickable = logic::pickable(task);
    if task.stage == Stage::Idea && app.task_draft.is_none() {
        any = true;
        row = row.push(text_button(
            "write-up",
            "Write up",
            Some(Message::EditTask),
            theme::button_primary,
        ));
    }
    if pickable {
        any = true;
        row = row.push(icon_text_button(
            "pick",
            Icon::Pick,
            Tone::OnPrimary,
            "Pick and copy prompt",
            app.picking.is_none().then_some(Message::Pick(task.number)),
            theme::button_primary,
        ));
    }
    for decision in logic::legal_decisions(task) {
        any = true;
        let (id, label) = match decision {
            Decision::Accept => ("accept", "Accept"),
            Decision::Reject => ("reject", "Reject"),
            Decision::Approve => ("approve", "Approve & build"),
            Decision::Replan => ("replan", "Replan"),
            Decision::Reopen => ("reopen", "Reopen"),
            Decision::MarkPlanned => ("mark-planned", "Mark planned"),
            Decision::MarkDone => ("mark-done", "Mark done"),
        };
        let message = Some(Message::Decide(task.number, decision));
        row = row.push(match decision {
            Decision::Reject => text_button(id, label, message, theme::button_danger),
            Decision::Accept | Decision::Approve => {
                text_button(id, label, message, theme::button_primary)
            }
            Decision::Replan | Decision::Reopen | Decision::MarkPlanned | Decision::MarkDone => {
                text_button(id, label, message, theme::button_secondary)
            }
        });
    }
    if task.claim.is_some() {
        any = true;
        row = row.push(text_button(
            "unclaim",
            "Unclaim",
            Some(Message::Unclaim(task.number)),
            theme::button_secondary,
        ));
    }
    any.then(|| row.wrap().vertical_spacing(SP_2).into())
}

fn plan<'a>(
    app: &'a App,
    task: &'a Task,
    editable: bool,
    list_height: f32,
) -> Element<'a, Message> {
    let ready = !task.plan.trim().is_empty();
    let mut head = row![
        icon(
            Icon::Plan,
            16.0,
            if ready { Tone::Ink } else { Tone::Muted }
        ),
        heading("Plan"),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);
    head = head.push(Space::new().width(Length::Fill));
    if ready && app.plan_draft.is_none() {
        head = head.push(icon_button(
            "copy-plan",
            Icon::Copy,
            Tone::Muted,
            "Copy plan as Markdown",
            Some(Message::CopyText(TaskText::Plan)),
            theme::button_ghost,
        ));
    }
    if editable && app.plan_draft.is_none() {
        head = head.push(widgets::icon_button(
            "edit-plan",
            Icon::Pencil,
            Tone::Muted,
            "Edit plan",
            Some(Message::EditPlan),
            theme::button_ghost,
        ));
    }
    let mut col = column![head].spacing(SP_2);
    if let Some(draft) = &app.plan_draft {
        return container(
            col.push(
                text_editor(draft)
                    .id("plan-editor")
                    .placeholder("Markdown")
                    .min_height(180)
                    .max_height(editor_max(list_height, PLAN_RESERVE, 180.0))
                    .padding(SP_2)
                    .size(TS_14)
                    .on_action(Message::PlanAction)
                    .style(theme::editor),
            )
            .push(
                row![
                    Space::new().width(Length::Fill),
                    text_button(
                        "cancel-plan-edit",
                        "Cancel",
                        Some(Message::CancelEditPlan),
                        theme::button_secondary
                    ),
                    text_button(
                        "save-plan",
                        "Save plan",
                        Some(Message::SavePlan),
                        theme::button_primary
                    ),
                ]
                .spacing(SP_2),
            ),
        )
        .id("plan")
        .into();
    }
    if ready {
        col = col.push(rich(app, &app.open_md.plan));
    } else {
        col = col.push(muted(
            "No plan yet. Once the task is accepted, an agent claims it and writes one.",
        ));
    }
    col.into()
}

fn priority(task: &Task) -> Element<'_, Message> {
    let segment = |id: &'static str, label: &'static str, value: Priority| {
        id_button(
            id,
            container(text(label).size(TS_12).font(MEDIUM)).padding([SP_1, SP_3]),
            Some(Message::SetPriority(value)),
            theme::button_segment(task.priority == value),
            Length::Shrink,
        )
    };
    row![
        heading("Priority").width(Length::Fill),
        segment("prio-high", "High", Priority::High),
        segment("prio-normal", "Normal", Priority::Normal),
        segment("prio-low", "Low", Priority::Low),
    ]
    .spacing(SP_1)
    .align_y(Alignment::Center)
    .into()
}

/// The project of the task; picking another moves the task there.
fn project<'a>(state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let choices: Vec<ProjectChoice> = state.projects.iter().map(ProjectChoice::of).collect();
    let selected = state
        .projects
        .iter()
        .find(|p| p.id == task.project)
        .map(ProjectChoice::of);
    row![
        heading("Project").width(Length::Fill),
        container(
            pick_list(choices, selected, |choice| match choice {
                ProjectChoice::Project { id, .. } => Message::MoveTask(id),
                ProjectChoice::New => Message::Noop,
            })
            .text_size(TS_14)
            .font(SEMIBOLD)
            .padding([SP_1, SP_2])
            .style(theme::picker)
            .menu_style(theme::picker_menu),
        )
        .id("task-project"),
    ]
    .spacing(SP_1)
    .align_y(Alignment::Center)
    .into()
}

fn notes<'a>(app: &'a App, state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let mut col = column![
        row![icon(Icon::Notes, 16.0, Tone::Muted), heading("Notes")]
            .spacing(SP_2)
            .align_y(Alignment::Center)
    ]
    .spacing(SP_3);
    if task.notes.is_empty() {
        col = col.push(muted(
            "No notes yet. Agents leave one when they finish or stop.",
        ));
    }
    for (i, note) in task.notes.iter().enumerate() {
        let mut who = row![member_tag(
            logic::member_name(&state.members, &note.author),
            logic::member_hue(&note.author)
        )]
        .spacing(6)
        .align_y(Alignment::Center);
        if let Some(agent) = &note.agent {
            who = who
                .push(icon(Icon::Bot, 14.0, Tone::Muted))
                .push(muted(agent));
        }
        who = who
            .push(Space::new().width(Length::Fill))
            .push(muted(format!(
                "{} ago",
                logic::ago(app.now_ms.saturating_sub(note.at_ms))
            )))
            .push(icon_button(
                format!("copy-note-{i}"),
                Icon::Copy,
                Tone::Muted,
                "Copy note as Markdown",
                Some(Message::CopyText(TaskText::Note(i))),
                theme::button_ghost,
            ));
        let body = match app.open_md.notes.get(i) {
            Some(md) => rich(app, md),
            None => text(&note.text).size(TS_14).into(),
        };
        col = col.push(column![who, body].spacing(SP_1));
    }
    let entered = !app.forms.note.trim().is_empty();
    col.push(
        row![
            text_input("Add a note", &app.forms.note)
                .id("note-input")
                .on_input(Message::NoteInput)
                .on_submit(Message::AddNote)
                .padding(SP_2)
                .size(TS_14)
                .style(theme::input),
            text_button(
                "add-note",
                "Add",
                entered.then_some(Message::AddNote),
                theme::button_secondary
            ),
        ]
        .spacing(SP_2)
        .align_y(Alignment::Center),
    )
    .into()
}
