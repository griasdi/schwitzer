//! The task dialog: what the task is, who works on it, its plan, the decisions and the notes.

use iced::widget::{
    Space, column, container, markdown, rich_text, row, span, text, text_editor, text_input,
};
use iced::{Alignment, Element, Font, Length};
use schwitzer_proto::{Decision, Priority, RoomState, Stage, Task};

use super::dialogs;
use super::icons::{Icon, Tone, icon};
use super::logic;
use super::widgets::{
    self, heading, hrule, icon_text_button, id_button, lamp, member_tag, muted, pill, text_button,
};
use super::{App, Message};
use crate::theme::{self, BOLD, MEDIUM, SP_1, SP_2, SP_3, TS_12, TS_14, TS_20};

pub fn view<'a>(app: &'a App, state: &'a RoomState, task: &'a Task) -> Element<'a, Message> {
    let editable = logic::editable(task);
    let header = row![
        lamp(task),
        text(format!("#{}", task.number))
            .size(TS_14)
            .font(MEDIUM)
            .style(theme::text_muted),
        pill(logic::stage_label(task)),
    ]
    .spacing(SP_2)
    .align_y(Alignment::Center);

    let mut body = column![
        title_and_body(app, task, editable),
        people(app, state, task)
    ]
    .spacing(SP_3);
    if let Some(actions) = actions(app, task) {
        body = body.push(actions);
    }
    body = body
        .push(hrule())
        .push(plan(app, task, editable))
        .push(hrule())
        .push(priority(task))
        .push(hrule())
        .push(notes(app, state, task));
    dialogs::frame("task-dialog", header.into(), body.into(), None)
}

fn title_and_body<'a>(app: &'a App, task: &'a Task, editable: bool) -> Element<'a, Message> {
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
        return column![
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
                .padding(SP_2)
                .size(TS_14)
                .on_action(Message::EditBody)
                .style(theme::editor),
            buttons,
        ]
        .spacing(SP_2)
        .into();
    }
    // Rich text, so the title is shown once in the widget inventory: the task list behind the
    // dialog carries the title as plain text, and addressing the task by it must stay
    // unambiguous.
    let title: Element<'a, Message> =
        rich_text![span::<(), Font>(task.title.as_str()).size(TS_20).font(BOLD)]
            .width(Length::Fill)
            .into();
    let mut top = row![title].spacing(SP_2).align_y(Alignment::Start);
    if editable {
        top = top.push(widgets::icon_button(
            "edit-task",
            Icon::Pencil,
            Tone::Muted,
            "Edit title and details",
            Some(Message::EditTask),
            theme::button_ghost,
        ));
    }
    let mut col = column![top].spacing(SP_2);
    if !task.body.is_empty() {
        col = col.push(text(&task.body).size(TS_14));
    }
    col.into()
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

/// Pick (when a finished plan waits) and the human decisions the server allows now.
fn actions<'a>(app: &'a App, task: &'a Task) -> Option<Element<'a, Message>> {
    let mut row = row![].spacing(SP_2).align_y(Alignment::Center);
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
        // Picking approves the plan already.
        if pickable && decision == Decision::Approve {
            continue;
        }
        any = true;
        let (id, label) = match decision {
            Decision::Accept => ("accept", "Accept"),
            Decision::Reject => ("reject", "Reject"),
            Decision::Approve => ("approve", "Approve plan"),
            Decision::Replan => ("replan", "Replan"),
            Decision::Reopen => ("reopen", "Reopen"),
        };
        let message = Some(Message::Decide(task.number, decision));
        row = row.push(match decision {
            Decision::Reject => text_button(id, label, message, theme::button_danger),
            Decision::Accept | Decision::Approve => {
                text_button(id, label, message, theme::button_primary)
            }
            Decision::Replan | Decision::Reopen => {
                text_button(id, label, message, theme::button_secondary)
            }
        });
    }
    if task.claim.is_some() {
        any = true;
        row = row.push(text_button(
            "release",
            "Release claim",
            Some(Message::Release(task.number)),
            theme::button_secondary,
        ));
    }
    any.then(|| row.wrap().vertical_spacing(SP_2).into())
}

fn plan<'a>(app: &'a App, task: &'a Task, editable: bool) -> Element<'a, Message> {
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
        return col
            .push(
                text_editor(draft)
                    .id("plan-editor")
                    .placeholder("Markdown")
                    .min_height(180)
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
            )
            .into();
    }
    if ready {
        // Links in a plan are not followed.
        col = col.push(
            markdown::view(app.plan_md.items(), theme::plan_markdown(&app.theme))
                .map(|_: markdown::Uri| Message::Noop),
        );
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
    for note in &task.notes {
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
            )));
        col = col.push(column![who, text(&note.text).size(TS_14)].spacing(SP_1));
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
