---
name: Schwitzer
description: A rack of raised slabs for coding agents and their humans; narrow, mint-and-green, every task a tactile plate with its own lamp, labels and next-step button.
colors:
  ground: "#e9f0ea"
  raised: "#ffffff"
  hover: "#f2f8f3"
  selected: "#e2f1e5"
  line: "#d9e5db"
  line-strong: "#75887a"
  text: "#142019"
  muted: "#56665a"
  primary: "#2db34a"
  primary-hover: "#27a142"
  on-primary: "#04210c"
  ink: "#1a7f35"
  danger: "#c53a35"
  danger-soft: "#fbe9e7"
  on-danger: "#ffffff"
  shadow: "rgba(12, 28, 17, 0.18)"
  edge: "rgba(12, 28, 17, 0.14)"
  stage-new: "#7d8c80"
  stage-idea: "#869589"
  stage-planning: "#c98410"
  stage-pickable: "#2f7fd6"
  stage-running: "#22a340"
  stage-closed: "#869589"
  stage-rejected: "#d9473f"
  tag-green-fill: "#d8f3dc"
  tag-green-text: "#14532d"
  tag-blue-fill: "#dbeafe"
  tag-blue-text: "#1e3a8a"
  tag-violet-fill: "#ede4fd"
  tag-violet-text: "#4c1d95"
  tag-orange-fill: "#ffe8cc"
  tag-orange-text: "#7c2d12"
  tag-pink-fill: "#fde2ef"
  tag-pink-text: "#831843"
  tag-teal-fill: "#d5f2f0"
  tag-teal-text: "#134e4a"
  night-ground: "#0d1310"
  night-raised: "#1a231e"
  night-hover: "#222d26"
  night-selected: "#213629"
  night-line: "#2a3a30"
  night-line-strong: "#6f8475"
  night-text: "#e4eee6"
  night-muted: "#93a597"
  night-primary: "#3dcb5c"
  night-primary-hover: "#5ad876"
  night-on-primary: "#03200b"
  night-ink: "#5ed77a"
  night-danger: "#f06a62"
  night-danger-soft: "#3a1a19"
  night-on-danger: "#2a0605"
  night-shadow: "rgba(0, 0, 0, 0.45)"
  night-edge: "rgba(0, 0, 0, 0.55)"
  night-stage-new: "#7d9182"
  night-stage-idea: "#6f8374"
  night-stage-planning: "#f0a630"
  night-stage-pickable: "#5aa6f5"
  night-stage-running: "#3dcb5c"
  night-stage-closed: "#738678"
  night-stage-rejected: "#f06a62"
  night-tag-green-fill: "#1d4a2a"
  night-tag-green-text: "#c6f6d0"
  night-tag-blue-fill: "#1e3a6b"
  night-tag-blue-text: "#d3e4ff"
  night-tag-violet-fill: "#3b2a6b"
  night-tag-violet-text: "#e6dcff"
  night-tag-orange-fill: "#5a3416"
  night-tag-orange-text: "#ffe2c4"
  night-tag-pink-fill: "#5c1f3d"
  night-tag-pink-text: "#ffd9ea"
  night-tag-teal-fill: "#16494a"
  night-tag-teal-text: "#cdf5f2"
typography:
  title:
    fontFamily: "Figtree"
    fontSize: "20px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "normal"
  heading:
    fontFamily: "Figtree"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "normal"
  body:
    fontFamily: "Figtree"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.3
    letterSpacing: "normal"
  body-medium:
    fontFamily: "Figtree"
    fontSize: "14px"
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: "normal"
  label:
    fontFamily: "Figtree"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "normal"
  caption-medium:
    fontFamily: "Figtree"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: "normal"
  caption:
    fontFamily: "Figtree"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.3
    letterSpacing: "normal"
rounded:
  lamp: "3px"
  control: "8px"
  pill: "999px"
spacing:
  sp-1: "4px"
  sp-2: "8px"
  sp-3: "12px"
  sp-4: "16px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.body-medium}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
    height: "32px"
  button-primary-hovered:
    backgroundColor: "{colors.primary-hover}"
    textColor: "{colors.on-primary}"
  button-primary-pressed:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.on-primary}"
  button-primary-disabled:
    backgroundColor: "{colors.line}"
    textColor: "{colors.muted}"
  button-secondary:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    typography: "{typography.body-medium}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
    height: "32px"
  button-secondary-hovered:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.text}"
  button-secondary-pressed:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.text}"
  button-danger:
    textColor: "{colors.danger}"
    typography: "{typography.body-medium}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
    height: "32px"
  button-danger-hovered:
    backgroundColor: "{colors.danger-soft}"
    textColor: "{colors.danger}"
  button-danger-pressed:
    backgroundColor: "{colors.danger}"
    textColor: "{colors.on-danger}"
  button-ghost:
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
  button-ghost-hovered:
    backgroundColor: "{colors.hover}"
  button-ghost-pressed:
    backgroundColor: "{colors.selected}"
  button-icon:
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "8px"
    size: "32px"
  button-segment:
    textColor: "{colors.text}"
    typography: "{typography.caption-medium}"
    rounded: "{rounded.control}"
    padding: "4px 12px"
  button-segment-on:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.ink}"
  task-slab:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    typography: "{typography.body-medium}"
    rounded: "{rounded.control}"
    padding: "8px 12px"
  task-slab-hovered:
    backgroundColor: "{colors.hover}"
  task-slab-pressed:
    backgroundColor: "{colors.selected}"
  task-slab-open:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
  group-header:
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "0 4px"
    height: "32px"
  group-header-hovered:
    backgroundColor: "{colors.hover}"
  group-header-pressed:
    backgroundColor: "{colors.selected}"
  session-slab:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "8px 12px"
  section:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "12px"
  lamp:
    backgroundColor: "{colors.stage-running}"
    rounded: "{rounded.lamp}"
    size: "10px"
  member-tag:
    backgroundColor: "{colors.tag-green-fill}"
    textColor: "{colors.tag-green-text}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "2px 8px"
  pill:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.muted}"
    typography: "{typography.caption-medium}"
    rounded: "{rounded.pill}"
    padding: "1px 8px"
  input:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "8px"
  header-bar:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    padding: "8px"
  page-header:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    typography: "{typography.title}"
    padding: "8px"
  status-bar:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    padding: "4px 8px"
    height: "32px"
  room-switcher:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    typography: "{typography.heading}"
    rounded: "{rounded.control}"
    padding: "4px 8px"
  room-switcher-hovered:
    backgroundColor: "{colors.hover}"
  room-switcher-opened:
    backgroundColor: "{colors.selected}"
  alert-strip:
    backgroundColor: "{colors.danger-soft}"
    textColor: "{colors.text}"
    typography: "{typography.caption}"
    rounded: "{rounded.control}"
    padding: "4px 8px"
---

# Design System: Schwitzer

## Overview

**Creative North Star: "The Slab Rack"**

The room is a rack of physical slabs. Every task is a raised, full-width plate lying in a recessed well; it carries its own stage lamp, its labels, the name tag of whoever works on it and one button for the next step. The construction is taken from the classic Steam client of the early 2000s: stacked full-width panels in a well, header bars, buttons and status labels inside the entries. The colours are not: the palette stays mint and green, the finish is clean and modern, Figtree is the typeface. It lives as a narrow window (400x780) beside the editor and terminal and is read in a glance, so everything is quiet and tinted toward green; the only saturated things on screen are the lamps, the name tags and the green action fills.

The system is tactile but calm. Depth is a pair of tones (the `ground` well is darker than every raised slab, by day and by night) plus a soft under-edge shadow that makes slabs and buttons look like plates lifted by a pixel or two. The shadow deepens under the pointer and vanishes when the plate is pressed in. Menus and tooltips float higher with a larger shadow. There is no motion by design: state changes are instant swaps, nothing animates, nothing blurs. Two themes, Daylight and Night, follow the system appearance (no preference means Night); every colour comes from the active theme's token set.

The signature is the press: a slab or button loses its under-edge shadow and darkens to `selected` the instant it is pressed. The second device is the stage lamp, a small square at the head of every task whose colour is the stage and whose hollow form marks the first two stages. It refuses the flat one-line contact list, the Kanban board and the wide three-pane admin layout.

**Key Characteristics:**
- Rack structure: a header bar, a recessed well of group headers and two-line slabs, sessions as slabs, a status bar; the next-step button sits on the slab it acts on.
- One saturated green for actions (`Action Green`), a separate leaf green for ink and focus, a separate running green for the lamp.
- Every neutral leans mint-green; no cool or pure greys.
- Six member tag hues, fixed per member, identical for everyone; light fills with dark text by day, dark fills with light text by night.
- Figtree only, four weights (400 / 500 / 600 / 700), three sizes in practice (12 / 14 / 20).
- Raised plates with a 1px hairline and a soft under-edge shadow; settings is an overview of slabs that each open a page with a back button, a task opens in place in the list and new tasks are drafted there too.

## Colors

A mint-and-green palette: a mint (day) or green-black (night) well, white or dark-green slabs above it, one action green, and a small spectrum of stage and member colours that carry all the meaning. Hex values below are Daylight / Night; the frontmatter is normative (`night-` prefixed keys).

### Primary
- **Action Green** (#2db34a / Night #3dcb5c): the action fill. Pick, Accept, Propose, Save, Approve, "Create or join a room", the selected entry in a pick-list menu. Text on it is **Deep Green Ink** (#04210c / #03200b, `on-primary`).
- **Action Green, Hovered** (#27a142 / Night #5ad876): hover fill for primary buttons.
- **Leaf Ink** (#1a7f35 / Night #5ed77a): green that reads as text and as the focus ring: the pressed primary button, `#4` task links in sessions, the "Share mine" link, the active segment of the priority control, input focus border, notice text in the status bar.

### Secondary (stage lamp colours)
Each is a lamp tint; none is used as a surface.
- **New, Outline Grey** (#7d8c80 / #7d9182): Proposed. Drawn as a *hollow* lamp.
- **Idea, Faint Grey** (#869589 / #6f8374, `stage-idea`): an idea not yet written up into a proposal. Hollow lamp, fainter than New in Night: an idea is less than a proposal. Idea and New are the only stages not filled.
- **Planning Amber** (#c98410 / #f0a630): Accepted and Planned without a ready plan; someone is writing or the plan is not final.
- **Pickable Blue** (#2f7fd6 / #5aa6f5): a plan is ready or approved and nobody builds it; the slab carries Approve on a ready plan, Pick on an approved one.
- **Running Green** (#22a340 / #3dcb5c): a claimed task, Accepted with a plan claim or Ready with a build claim; someone is working. Also the online dot.
- **Closed Muted** (#869589 / #738678): Done. Same day value as Idea; the filled form tells them apart.
- **Rejected Red** (#d9473f / #f06a62): Rejected.
- **Alarm Red** (#c53a35 / #f06a62, `danger`): destructive buttons, the high-priority pill, offline dots, the alert strip border. Soft fill **Alarm Wash** (#fbe9e7 / #3a1a19, `danger-soft`); text on a pressed danger button **#ffffff / #2a0605**.

Lamp colours hold 3:1 on `raised` in both themes.

### Tertiary (member name tags)
Six hues, picked per member by a fixed hash, the same on every machine. Day: light fill + dark text of the same hue; Night: dark fill + light text.
- **Tag Green** fill #d8f3dc, text #14532d (Night #1d4a2a / #c6f6d0)
- **Tag Blue** fill #dbeafe, text #1e3a8a (Night #1e3a6b / #d3e4ff)
- **Tag Violet** fill #ede4fd, text #4c1d95 (Night #3b2a6b / #e6dcff)
- **Tag Orange** fill #ffe8cc, text #7c2d12 (Night #5a3416 / #ffe2c4)
- **Tag Pink** fill #fde2ef, text #831843 (Night #5c1f3d / #ffd9ea)
- **Tag Teal** fill #d5f2f0, text #134e4a (Night #16494a / #cdf5f2)

### Neutral
- **Mint Well / Night Well** (#e9f0ea / #0d1310, `ground`): the recessed well behind the slabs: list, sessions, page bodies, and the fill of pills and disabled inputs. Always darker than `raised`.
- **Slab White / Night Slab** (#ffffff / #1a231e, `raised`): task slabs (open or not), session slabs, page sections, the header, page and status bars, inputs, tooltips, menus, secondary buttons.
- **Mint Breath** (#f2f8f3 / #222d26, `hover`): a slab or button under the pointer.
- **Mint Pressed** (#e2f1e5 / #213629, `selected`): a pressed slab or button, text selection, active segment fill, opened room switcher.
- **Mint Hairline** (#d9e5db / #2a3a30, `line`): separators, slab and resting input borders, disabled secondary-button borders, scroller rails, disabled primary fill.
- **Sage Edge** (#75887a / #6f8475, `line-strong`): resting secondary-button and segment borders, hovered slab and input borders, open-task border, unknown-state dot, scroller while hovered; 3:1 on `ground`, `raised`, `hover` and `selected`.
- **Green-Black Ink / Night Text** (#142019 / #e4eee6, `text`): all primary text.
- **Moss Grey** (#56665a / #93a597, `muted`): secondary text, placeholders, inactive icons; held at 4.5:1 on `ground`, `raised`, `hover` and `selected`.
- **Under-Edge** (rgba(12, 28, 17, 0.14) / rgba(0, 0, 0, 0.55), `edge`): the soft shadow of raised slabs and buttons.
- **Shadow** (rgba(12, 28, 17, 0.18) / rgba(0, 0, 0, 0.45)): the shadow of floating cards (menu, tooltip).

### Named Rules
**The Lamp Carries Status Rule.** Stage is shown by the lamp colour and nowhere else on a slab: no slab backgrounds, side stripes or coloured titles. A stage colour is never used for anything but a stage. The stage label (a pill) and the lamp say the same thing twice.

**The Three Greens Rule.** Green means three different things and they do not swap: *Action Green* fills actions, *Leaf Ink* writes text and the focus ring, *Running Green* marks a running agent, a lamp or a live connection. An action never takes the running hue and a lamp never takes the action hue.

**The Mint Neutrals Rule.** Every neutral is tinted toward the green hue; the well is mint, lines are mint, text is green-black. No pure grey, no blue-grey.

**The Colour Plus Form Rule.** Colour is never the only carrier. The lamp sits next to a title and every slab carries a stage label ("Proposed", "Plan ready", "Building"); Idea and New are hollow, not just grey. Name tags carry the member's name; the priority pill names High and Low.

## Typography

**Display Font:** Figtree (bundled, no fallback needed; the app sets it as the default font)
**Body Font:** Figtree
**Label/Mono Font:** Figtree (no monospace; inline code in plans keeps the UI face)

**Character:** Figtree is a friendly geometric sans with slightly rounded terminals, the right register for a tool that sits beside the editor all day. Hierarchy comes from weight and from three sizes, not from many faces or tracking (iced has no letter spacing; all tracking is normal).

### Hierarchy
- **Title** (600, 20px, 1.3): page titles ("Settings"); markdown `h1` in task details, plans and notes.
- **Heading** (600, 14px, 1.3): a block heading inside a page section or the open task ("Plan", "Priority", "Notes", "Your name"); the room switcher.
- **Body** (400, 14px, 1.3): task details, inputs, session titles, plan text.
- **Body Medium** (500, 14px, 1.3): task titles on slabs and in the head of the open task, button labels.
- **Label** (600, 12px, 1.3): group headers, "Sessions", member name tags, the status-bar user name.
- **Caption Medium** (500, 12px, 1.3): `#n` on slabs, pills, session runtime, segment labels.
- **Caption** (400, 12px, 1.3, Moss Grey): hints, project names, token counts, tooltips, empty-state hints.

Line height is iced's default relative 1.3 everywhere; no override.

### Named Rules
**The Three Sizes Rule.** 12 for captions, tags and counters; 14 for anything read as content; 20 for titles. Task markdown steps `h2` 17 and `h3` 15 between them; nothing else invents a size.

**The Weight Does the Work Rule.** Emphasis is a weight step (400, 500, 600, 700), never a size jump, an underline or tracking.

## Layout

A single narrow column, designed for a 400x780 window (minimum 340x520), pinned beside the editor. The window shows one **page** at a time above a 1px hairline and the 32px **status bar**, which stays on every page; an optional dismissable error strip (8px inset) tops all of it.

**Main page**, top to bottom: the **header bar** (room switcher left; invite and settings ghost icons right; 8px padding), a hairline, the **task list** (60% of the area, `FillPortion(60)`), a hairline, the **sessions** panel (38%, `FillPortion(38)`).

**Pages** replace the main column: a header bar with a ghost back button (`Esc` tooltip) and the title (20px semibold), a hairline, a scrolling body, an optional footer bar (hairline, 8px 12px padding, right-aligned Cancel + primary action). The body is centred, at most 560px wide, 12px inset; its blocks are **sections**, one raised slab each, 16px apart. Every page opens at its top. A task is not a page: it opens in place in the list, and a new task is drafted there (see Slabs).

**Settings** is an overview of **nav slabs** grouped by scope, each group under a 12px semibold label inset 4px, groups 12px apart and slabs 4px apart: Room (the selected room, the selected project, all rooms), This machine (omp, projects folders, updates), You (the name in a section and the notification switch as a slab), then About on its own. Each nav slab opens the page of its topic, whose back button (and `Esc`) returns to the overview; the overview's returns to the main page. A danger action (leave or remove a room, delete a project) asks in place: its button becomes the question, Cancel and the danger action; `Esc` cancels.

Spacing is a four-step scale: 4 / 8 / 12 / 16 (`sp-1`..`sp-4`). The list well is inset 8px; groups stack 12px apart, slabs inside a group 4px apart. The scrollable embeds its rail beside the slabs (4px), so slabs span the full width and the rail never overlaps them. Controls are 32px tall (text buttons, icon buttons at 16px glyph + 8px padding, group headers, status bar, sessions header).

The list is grouped in fixed order: **In progress** (every claimed task, planning or building), **Ready to pick**, **Planning**, **Proposals**, **Ideas**, **Done**. **Proposals** and **Ideas** are always shown, even empty, so their `+` stays reachable; the other groups disappear when empty. Groups fold with a chevron; within a group, higher priority first, then task number. A slab is two lines plus the action: line one, lamp (10px square) and the title (fills, wraps); line two, indented under the title: `#n`, the stage pill, the priority pill (High or Low; nothing for Normal), spacer, the claimer's name tag. At the right edge, vertically centred, the one next-step button. A wide window changes nothing except that pages grow to at most 560px wide.

Empty states are centred, max 280px: a 40px Lucide glyph (plug while connecting, users on the welcome screen, muted), a semibold title, a muted hint, one action. An empty room shows its pinned groups and a muted hint line.

Follows the system appearance: light = Daylight, dark (or unknown) = Night.

## Elevation & Depth

Tactile by tone and edge. The well sits below, slabs and bars sit above it as `raised`; a plate is a 1px `line` border plus a soft under-edge shadow. A plate that is pressed loses the shadow and darkens. Floating things (menu, tooltip) cast bigger shadows. There is no glass, blur or gradient.

### Shadow Vocabulary
- **Slab / button, at rest** (`offset 0 1px, blur 2px, colour edge`): task slabs, the open task and the draft, session slabs, page sections, primary / secondary / danger buttons, inactive segments.
- **Slab / button, hovered** (`offset 0 2px, blur 4px, colour edge`): the plate lifts a little more, together with the `hover` fill and the `line-strong` border on slabs.
- **Pressed and disabled**: no shadow. The active segment of a segmented control is also shadowless: it reads as pressed in.
- **Menu** (`offset 0 4px, blur 16px, colour shadow`): the room-switcher menu.
- **Tooltip** (`offset 0 2px, blur 8px, colour shadow`): hover tips above the control, 4px gap; below it for the icon buttons of the header and page bars, which sit at the window's top edge.

Flat on purpose: bars (header, page, status; separated by 1px hairlines), ghost buttons, group headers, flat rows inside a slab (the room list in Settings), name tags, pills, lamps, inputs.

### Named Rules
**The Press Rule.** Raised means pressable: a slab or button loses its under-edge shadow and darkens to `selected` when pressed. Nothing that cannot be pressed is lifted except a section or session slab, which carries only the resting shadow and never reacts.

**The No Slabs in Slabs Rule.** A raised plate never contains another raised plate with its own shadow, except the action button a slab carries. Rows inside a section (rooms, repos) are flat, hover-tinted rows.

## Shapes

Soft but not bubbly. Everything interactive is a rounded rectangle with an 8px radius (`control`): slabs, sections, buttons, inputs, editors, the room switcher, menus, alert strip. Stage lamps are small squares with a 3px radius (`lamp`), 10px wide; Idea and New are hollow with a 2px border, all other stages filled. Name tags, pills, dots and scroller rails are fully round (999px). Tooltips use 6px and inline code 4px (one-offs, see below). Borders are 1px hairlines in `line`; focus is 2px. Icons are Lucide (stroke, ISC) at 14px in group headers, 16px on buttons, 12px in the priority pill, 40px in empty states, tinted by theme tokens.

## Components

### Buttons
Four kinds plus a segmented control; every button is wrapped in an id container so tests and the headless control can address it.
- **Shape:** 8px radius; 32px tall (6px 12px padding around a 14px medium label; icon+label gap 6px, icon 16px). Primary, secondary and danger buttons are raised plates: under-edge shadow at rest, deeper on hover, none when pressed or disabled.
- **Primary:** Action Green fill, Deep Green Ink text. Hovered: `primary-hover`. Pressed: Leaf Ink fill. Disabled: Hairline fill, Moss Grey text. One per view for the main call to action (Propose, Save, "Approve & build", "Pick and copy prompt"); on a slab it is the next step: **Approve** (check icon) on a ready plan, **Pick** (hand-grab icon) on an approved one, **Accept** (check icon) on proposals. Pick is disabled (muted icon) while a pick runs. Right after an approval Approve stays on the slab, disabled with a muted icon, for up to 6 s while the planning omp session claims the build, then Pick takes its place.
- **Secondary:** raised fill, Sage Edge border; hovered Mint Breath with Moss Grey border; pressed Mint Pressed with Moss Grey border; disabled: no fill, Mint Hairline border, Moss Grey text. Cancel, Replan, Reopen, Mark planned, Mark done, Add, Save name; **Unclaim** (leave icon) on a slab whose claim is yours.
- **Danger:** red text and 1px red border on no fill; hovered Alarm Wash; pressed solid red with on-danger text. Reject (open task only).
- **Ghost:** no border, no fill, no shadow; hover Mint Breath, pressed Mint Pressed. Icon buttons (32x32, 16px glyph, tooltip above, below in the header and page bars) in the bars (invite, settings, back), group headers (`+`) and the head of the open task (edit, close), text links ("Share mine"), the `#n` session task link.
- **Segment:** 1px Sage Edge border, Moss Grey under the pointer; the active segment is Mint Pressed with Leaf Ink border and text, shadowless (priority High / Normal / Low).
- **Switch:** an on/off setting. Always inside a full-width row button (a slab on the Settings overview, a flat row inside a section) with the label and a muted caption on the left and the switch on the right; a press anywhere on the row flips it. Track 20px tall, fully round, 1px border, knob `raised` at 15% inset. On: Action Green track and border, borderless knob. Off: `ground` track, Sage Edge track border and knob border.

### Slabs and group headers
- **Task slab:** a full-width button on `raised`, 8px radius, 1px Mint Hairline border, under-edge shadow, padding 8px 12px. Hover: Mint Breath fill, Sage Edge border, deeper shadow. Pressed: Mint Pressed, no shadow. Opens the task in place. Two lines plus the next-step button (see Layout); the button takes the press before the slab does. Reject, Replan, Reopen, Mark planned and Mark done stay in the open task.
- **Nav slab:** the task slab's look on the Settings overview: 16px muted icon, title (14px medium) over a status line (12px), 16px muted chevron right, 12px between them. The status line is muted when all is well, Leaf Ink when something wants a look (an out-of-date plugin, an update to install), red with a 12px alert icon when something is broken (server not answering, port in use).
- **Open task:** a press on a slab opens it in place; only one element is open. Head = the slab's two lines on a flat hover-tinted row (press closes), with ghost Edit and Close (`Esc`) icons; under a hairline, at 12px padding: details, people, action row, Plan, Priority, Notes, with hairlines between. Resting shadow, Sage Edge border; opening scrolls it into view. Once its top scrolls past the list's top, the head and its hairline pin there on a square `raised` plate with Sage Edge sides, until the slab's bottom pushes them out. The details and plan editors grow up to the list's height less their controls, then scroll inside, so Cancel and Save stay on screen.
- **Draft:** `+` (or `Ctrl+N` for a proposal) opens a draft at the top of Proposals or Ideas: hollow lamp + title field, details editor (capped like the open task's), Cancel + Propose / Add idea. Esc, Cancel or opening another element closes it and keeps its text.
- **Group header:** a flat full-width button in the well, 32px tall: chevron (14px muted), title (12px semibold), count pill, spacer, and on Proposals and Ideas a ghost `+` (tooltips "New proposal  (Ctrl+N)" and "New idea") that opens a draft at the top of that group. Hover Mint Breath, pressed Mint Pressed; the `+` takes the press before the header.
- **Section / session slab:** the resting task slab as a plain container (hairline border, under-edge shadow), never reacting. Sections hold page blocks at 12px padding; session slabs, 8px 12px.

### Lamps, name tags and pills
- **Lamp:** 10px square, 3px radius; filled in the stage colour, hollow (2px border) for Idea and New.
- **Name tag:** fully round, 2px 8px padding, 12px semibold; fill/text from the member's tag hue. Appears on slabs with a claim, in sessions, in notes.
- **Pill:** quiet round label on the Mint Well with a Hairline border and Moss Grey 12px medium text: the stage label on a slab and in the head of the open task, group counts, "idle" on a session.
- **Priority pill:** a pill with a 12px arrow and label: High (red arrow, red text), Low (muted arrow and text); none for Normal.
- **Dot:** 8px round; running green up, red down, Sage Edge unknown. Server and agent port state in the status bar.

### Inputs / Fields
- **Style:** raised fill, 1px Mint Hairline border, 8px radius, 8px padding, 14px text, muted placeholder; text editors the same.
- **Hover:** border becomes Sage Edge.
- **Focus:** border becomes Leaf Ink at 2px (the focus ring); selection uses Mint Pressed.
- **Disabled:** Mint Well fill.
- **Enter** submits the field's action (propose, save, add note, save name).

### Navigation: header bar, status bar, room switcher, pages
- **Header bar (main page):** raised, 8px padding. Left: the **room switcher**, or a muted "No room". Right: ghost invite icon (copy room key; disabled and muted without a room) and ghost settings icon with `Ctrl+,` tooltip. No button bar and no "New task" button: tasks are added with the `+` in the Proposals and Ideas group headers.
- **Room switcher:** a pick list in the header bar, 14px semibold, flat on the raised bar until hovered (Mint Breath, Sage Edge border); opened = Mint Pressed with Leaf Ink border. Its menu is raised, hairline-bordered, 8px radius, selected entry in Action Green with on-primary text.
- **Page header:** raised bar, 8px padding: ghost back button (tooltip "Back  (Esc)") and the title (20px semibold). Below it a hairline; footer, if any, is a raised bar under a hairline.
- **Status bar:** 32px, raised, padding 4px 8px, on every page. Left to right: my name (12px semibold), spacer, server dot + "online/offline/connecting" (or a transient notice in Leaf Ink), agent port dot + "agent port". Both indicators have tooltips with detail.

### Sessions
A 32px header ("Sessions" in the group-header style, count pill, optional ghost "Share mine"), then one static slab per session, 4px apart in 8px inset: member tag, session title (14px), a caption line (project, `#n` task link in Leaf Ink, tokens, "idle" pill) and the runtime on the right. Slabs are not buttons; they scroll with the rail beside them.

### Scroll rails
Thin pill scrollers only, no track: Mint Hairline at rest, Sage Edge when hovered, Moss Grey while dragged. In the list, sessions and pages the rail sits beside the content, not over it.

### Alert strip
Alarm Wash fill, 1px red border, 8px radius; alert icon, 12px text, ghost dismiss. Sits at the very top, 8px from the window edge, above every page.

### Motion
None, by design. Hover, press and focus are instant swaps (including the shadow); pages, the open task and the draft appear and disappear without transition; there are no animated indicators (the runtime counters tick once a second as text).

The product site in `site/` is the one exception: it borrows the tokens, Figtree and the slab, lamp and tag components, but animates on purpose, because its job is to show the room working. Its motion is a single authored moment, the two-desk demo: a packet in the new stage's lamp colour crosses the room server (two 480 ms hops), slabs move to their new group (420 ms), all on `cubic-bezier(0.16, 1, 0.3, 1)`, and building slabs shed small sweat drops. Everything else on the page is static, and `prefers-reduced-motion` turns the demo into a click-through without travel, slab movement or drops. The page has its own larger type and spacing scale (display, section titles, 16px section radius); the replica windows inside it keep the app's metrics.

### Accessibility and input
iced 0.14 exposes no accessibility tree and screen readers cannot read the window, so contrast and keyboard operation carry accessibility. Muted text is held at 4.5:1 on every surface; Sage Edge (resting secondary and segment borders, hovered slab and input borders) holds 3:1 on every surface; lamps hold 3:1 on raised. Keyboard: `Tab` / `Shift+Tab` move focus through the fields, `Esc` cancels a pending confirmation, else closes the open task or draft, else goes back one page (a settings topic to Settings, Settings to the main page), `Ctrl+N` (Cmd on macOS) opens the draft of a new proposal, `Ctrl+,` opens the Settings page, `Enter` submits the focused field. Focus is visible only on fields and editors (2px Leaf Ink ring): iced 0.14 buttons have no focused state, so buttons and slabs are pointer targets (at least 32px tall) and every keyboard path must go through a shortcut or a field. Every icon-only button has a tooltip naming its action.

## Do's and Don'ts

### Do:
- **Do** express stage only with the lamp colour from the stage tokens (`stage-new` and `stage-idea` hollow, `stage-planning`, `stage-pickable`, `stage-running`, `stage-closed`, `stage-rejected`) plus the stage label.
- **Do** put the next step on the slab it acts on: at most one button (Approve, Pick, Accept or Unclaim) per slab.
- **Do** use `primary` for the main action of a view (or a slab's next step) and `ink` for green text, links and the focus ring.
- **Do** keep every neutral mint-tinted: well `#e9f0ea` / `#0d1310`, lines `#d9e5db` / `#2a3a30`, text `#142019` / `#e4eee6`.
- **Do** keep the well darker than the slabs on it, in both themes.
- **Do** read colours from the active theme's tokens through the style functions in `theme.rs`; add a new colour as a token pair (Daylight and Night) first.
- **Do** keep controls at 32px height and slabs at 8px radius, 8px 12px padding, 4px apart.
- **Do** put a tooltip on every icon-only button and name the shortcut where one exists (`Settings  (Ctrl+,)`, `Back  (Esc)`, `Close  (Esc)`).
- **Do** make Settings an overview of nav slabs whose topics are pages with a back button; open tasks and drafts in place in the list, one at a time.
- **Do** write weight, not size, to emphasise: 400 / 500 / 600 / 700 in 12 / 14 / 20.

### Don't:
- **Don't** recolour slabs, tint backgrounds or add side stripes to show status; the lamp is the status.
- **Don't** introduce a Kanban board, a wide multi-pane layout or any pane beside the list; new full surfaces are pages with a back button, small ones open in place in the list.
- **Don't** add a button bar or a standing "New task" button; the `+` in a group header adds tasks.
- **Don't** use Action Green for decoration, or Running Green for a button.
- **Don't** lift anything that cannot be pressed beyond a section or session slab's resting shadow; no shadow on bars, ghost buttons, headers or pills; no gradients, blur or glass.
- **Don't** nest slabs: rows inside a section stay flat.
- **Don't** add animation or transitions to the app; it is motionless by design (the product site's demo is the documented exception).
- **Don't** use pure grey, pure black or a cool grey; the Night well is green-black `#0d1310`, not `#000000`.
- **Don't** use a second typeface, tracking, or sizes beyond 12 / 14 / 20 (plus plan headings 15 / 17).
- **Don't** rely on colour alone: a stage needs the lamp form (hollow for Idea and New) and a label; a person needs a named tag.
- **Don't** assume buttons are keyboard focusable in iced 0.14; every action reachable only by button needs a shortcut or an obvious pointer path.

### Not canonized (defects the build carries)
- Layout literals outside the token scale: 6px icon-label gaps and button padding, 2px gaps in session and repo rows, 10px lamp, 1px/2px pill and tag padding, tooltip radius 6px, inline-code radius 4px, plan heading sizes 15 / 17. They are one-offs of the build, kept above for fidelity but not extended to new surfaces.
- No visible focus indicator on buttons, slabs and the room switcher (a limit of iced 0.14's button status).
