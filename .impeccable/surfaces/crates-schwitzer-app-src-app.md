---
version: 1
slug: "crates-schwitzer-app-src-app"
primary_target: "crates/schwitzer-app/src/app"
related_targets: []
---

Scope: the whole schwitzer-app window (task list, sessions, header bar, status bar, pages, task dialog). Mode: Operate.

Audience/job: two developers glancing at the room beside their editor: who works on what, which plan is ready to pick, which agent sessions run. Tasks: propose, decide, pick a finished plan (claim + prompt copy), configure rooms and repos.

Constraints: narrow window (default 400x780); vertical split 60% task list, 38% sessions, header bar on top, bottom status bar with server state. New task and settings are pages with a back button; the task view is a dialog. Follow system light/dark.

## Direction contract

seed: none (direction pinned by the user: "Bauart wie das klassische Steam, Farben nicht übernehmen, clean und modern")

THESIS: the room is a rack of physical slabs: every task is a raised full-width plate in a recessed well, carrying its own lamp, labels and next-step button. Refuses the flat buddy list and the Kanban board.

OWN-WORLD: existing mint/green tokens; recessed `ground` well, white/green-black raised slabs with a hairline border and a soft 1px under-edge shadow that disappears when pressed; header and status bars as raised panels; Figtree; 8px radius; square stage lamps.

STORY: open the app, scan "In progress" slabs with lamps, labels and name tags; press Accept or Pick right on the slab; `+` in a group header opens the New task page.

FIRST VIEWPORT: header bar (room switcher left, invite + settings right), list well with group headers and two-line slabs (60%), sessions slabs (38%), status bar.

FORM: built-in iced widgets; slabs are styled buttons/containers; pages replace the main column, the task dialog overlays it.

SIGNATURE: the press: a slab or button loses its under-edge shadow and darkens to `selected` when pressed.
