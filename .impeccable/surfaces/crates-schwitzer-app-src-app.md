---
version: 1
slug: "crates-schwitzer-app-src-app"
primary_target: "crates/schwitzer-app/src/app"
related_targets: []
---

Scope: the whole schwitzer-app window (task list, sessions, toolbar, status bar, dialogs, settings). Mode: Operate.

Audience/job: two developers glancing at the room beside their editor: who works on what, which plan is ready to pick, which agent sessions run. Tasks: propose, decide, pick a finished plan (claim + prompt copy), configure rooms and repos.

Constraints: narrow window (default 400x780); vertical split 60% task list, 38% sessions, slim button bar, bottom status bar with room switcher and server state. New task, task view and settings are dialogs. Follow system light/dark.

## Direction contract

seed: none (direction pinned by the user: "moderner ICQ style")

THESIS: the room is a buddy list. Tasks are contacts with a status flower; groups fold like ICQ groups. Refuses the Kanban board and the wide three-pane admin layout.

OWN-WORLD: fresh ICQ green on white (light) or deep green-black (dark), mint-tinted chrome for toolbar and status bar, Figtree type, rounded 8px rows, coloured name tags per member, one eight-petal status flower whose colour is the stage.

STORY: open the app, read "In progress" with name tags, see "Ready to pick" with plan-ready flowers, pick one; the prompt lands in the clipboard for your agent.

FIRST VIEWPORT: list fills the top 60%, grouped and collapsible; sessions panel below with member tags and runtime/tokens; icon toolbar (new task primary, show done, copy key, settings); status bar: flower, my name, room picker, server dot, MCP dot.

FORM: standard iced controls, list rows, pick list, modal dialogs with scrim.

SIGNATURE: the stage-coloured status flower on every row and in the status bar.
