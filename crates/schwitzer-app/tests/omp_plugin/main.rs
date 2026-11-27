//! The omp plugin end to end: the real headless app installs it into a repo, a real
//! `omp --mode rpc` loads it and talks to a scripted model, and bob, a second member of the room,
//! makes the human decisions. Needs `omp` on `PATH`; see AGENTS.md, "Testing the omp plugin".

mod fake_model;
mod omp;
mod world;

use schwitzer_proto::{Stage, Work};
use serde_json::json;

use fake_model::Reply;
use world::World;

const MARKER: &str = "schwitzer-claim: ";

/// The claim token after the marker in `text`, checked to be 32 lowercase hex digits.
fn token(text: &str) -> String {
    let (_, rest) = text
        .split_once(MARKER)
        .unwrap_or_else(|| panic!("no claim marker in {text:?}"));
    let token: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    assert!(
        token.len() == 32
            && token
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "bad claim token {token:?}"
    );
    token
}

#[test]
fn plan_picks_a_task_and_claims_it_for_planning() {
    let mut w = World::start();
    let mut omp = w.omp();
    assert_eq!(w.accepted("First", "Body one."), 1);
    assert_eq!(w.accepted("Second", "Body two."), 2);

    let id = omp.prompt("/schwitzer plan");
    let (dialog, options) = omp.select("Plan a Schwitzer task");
    assert_eq!(options, ["#1 First", "#2 Second"]);
    omp.answer(&dialog, "#2 Second");
    let text = omp.editor_text();
    assert!(
        text.starts_with(
            "/plan Plan Schwitzer task #2 \"Second\" from project \"demo\" in room \"crew\". schwitzer-claim: "
        ),
        "editor text {text:?}"
    );
    omp.notified("Task #2 is claimed for planning. Press Enter to start.");
    omp.finished(&id);

    let task = w.task_where(2, "claimed", |t| t.claim.is_some());
    let claim = task.claim.expect("claim");
    assert_eq!(claim.work, Work::Plan);
    assert_eq!(claim.agent, "omp");
    assert_eq!(claim.token.to_hex(), token(&text));
    assert_eq!(w.task_where(1, "present", |_| true).claim, None);
}

#[test]
fn build_reaches_the_agent_and_finish_offers_the_next_build() {
    let mut w = World::start();
    let mut omp = w.omp();
    assert_eq!(w.approved("Add logging", "Log every request."), 1);
    assert_eq!(w.approved("Second", "Body two."), 2);

    let id = omp.prompt("/schwitzer build 1");
    let text = omp.editor_text();
    assert!(
        text.starts_with(
            "Build Schwitzer task #1 \"Add logging\" from project \"demo\" in room \"crew\". schwitzer-claim: "
        ),
        "editor text {text:?}"
    );
    token(&text);
    omp.notified("Task #1 is claimed for building. Press Enter to start.");
    omp.finished(&id);

    // The user presses Enter on the editor text; the agent starts on it.
    w.model.push(Reply::Text("Working on it.".into()));
    let id = omp.prompt(&text);
    omp.finished(&id);
    // The claim line links the session while the build is claimed; the scanner runs every 5 s.
    let bob = w.bob.member();
    w.state_where("the omp session linked to #1", |state| {
        state
            .sessions
            .iter()
            .any(|(member, infos)| *member != bob && infos.iter().any(|info| info.task == Some(1)))
    });

    w.model.push(Reply::Tool {
        name: "schwitzer_finish".into(),
        args: json!({"room": "crew", "task": 1, "note": "Logged every request."}),
    });
    w.model.push(Reply::Text("Done.".into()));
    let id = omp.prompt("Finish it.");
    let end = omp.tool_end("schwitzer_finish");
    assert_eq!(end["isError"], false, "schwitzer_finish: {end}");
    omp.finished(&id);

    let requests = w.model.requests();
    assert!(
        requests[0].contains("Log every request."),
        "the task did not reach the model: {requests:?}"
    );
    let task = w.task_where(1, "done", |t| t.stage == Stage::Done);
    let note = task.notes.last().expect("a note");
    assert!(note.text.contains("Logged every request."), "note {note:?}");

    let (dialog, options) = omp.select("Build #2 \"Second\" next?");
    assert_eq!(
        options,
        [
            "In a fresh session (recommended)",
            "In this session",
            "Not now"
        ]
    );
    omp.answer(&dialog, "Not now");
}

#[test]
fn given_back_build_offers_a_new_plan() {
    let mut w = World::start();
    let mut omp = w.omp();
    assert_eq!(w.approved("Add logging", "Log every request."), 1);

    w.model.push(Reply::Tool {
        name: "schwitzer_unclaim".into(),
        args: json!({"room": "crew", "task": 1, "note": "the plan misses the cache"}),
    });
    w.model.push(Reply::Text("Gave it back.".into()));
    let id = omp.prompt("/schwitzer build 1 now");
    omp.notified("Task #1 is claimed for building.");
    omp.finished(&id);
    let end = omp.tool_end("schwitzer_unclaim");
    assert_eq!(end["isError"], false, "schwitzer_unclaim: {end}");

    let (dialog, options) = omp.select(
        "#1 \"Add logging\" was given back: the plan misses the cache. Send it back for a new plan?",
    );
    assert_eq!(
        options,
        [
            "Send it back and plan it here",
            "Send it back only",
            "Leave it for the room"
        ]
    );
    omp.answer(&dialog, "Send it back only");
    omp.notified("Schwitzer: #1 sent back for a new plan.");
    omp.notified("Run /schwitzer plan 1 to plan it.");

    w.task_where(1, "accepted and unclaimed", |t| {
        t.stage == Stage::Accepted && t.claim.is_none()
    });
}
