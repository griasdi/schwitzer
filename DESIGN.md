---
name: Schwitzer
description: A fresh ICQ buddy list for coding agents and their humans; narrow, mint-and-green, every task a contact with a status flower.
colors:
  ground: "#ffffff"
  chrome: "#edf5ee"
  raised: "#ffffff"
  hover: "#f2f8f3"
  selected: "#e2f1e5"
  line: "#d9e5db"
  line-strong: "#8fa394"
  text: "#142019"
  muted: "#56665a"
  primary: "#2db34a"
  primary-hover: "#27a142"
  on-primary: "#04210c"
  ink: "#1a7f35"
  danger: "#c53a35"
  danger-soft: "#fbe9e7"
  on-danger: "#ffffff"
  scrim: "rgba(12, 28, 17, 0.36)"
  shadow: "rgba(12, 28, 17, 0.18)"
  stage-new: "#7d8c80"
  stage-planning: "#c98410"
  stage-pickable: "#2f7fd6"
  stage-running: "#22a340"
  stage-closed: "#9aa89d"
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
  night-ground: "#121915"
  night-chrome: "#0c110e"
  night-raised: "#1a231e"
  night-hover: "#1b2620"
  night-selected: "#213629"
  night-line: "#2a3a30"
  night-line-strong: "#627568"
  night-text: "#e4eee6"
  night-muted: "#93a597"
  night-primary: "#3dcb5c"
  night-primary-hover: "#5ad876"
  night-on-primary: "#03200b"
  night-ink: "#5ed77a"
  night-danger: "#f06a62"
  night-danger-soft: "#3a1a19"
  night-on-danger: "#2a0605"
  night-scrim: "rgba(0, 0, 0, 0.55)"
  night-shadow: "rgba(0, 0, 0, 0.45)"
  night-stage-new: "#7d9182"
  night-stage-planning: "#f0a630"
  night-stage-pickable: "#5aa6f5"
  night-stage-running: "#3dcb5c"
  night-stage-closed: "#5f7064"
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
  headline:
    fontFamily: "Figtree"
    fontSize: "20px"
    fontWeight: 700
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
  control: "8px"
  dialog: "14px"
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
  button-pick:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "4px 10px"
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
  list-row:
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "6px 8px"
  list-row-hovered:
    backgroundColor: "{colors.hover}"
  list-row-pressed:
    backgroundColor: "{colors.selected}"
  group-header:
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "4px"
  member-tag:
    backgroundColor: "{colors.tag-green-fill}"
    textColor: "{colors.tag-green-text}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "2px 8px"
  pill:
    backgroundColor: "{colors.chrome}"
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
  toolbar:
    backgroundColor: "{colors.chrome}"
    textColor: "{colors.text}"
    padding: "4px 8px"
  status-bar:
    backgroundColor: "{colors.chrome}"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    padding: "4px 8px"
    height: "32px"
  room-switcher:
    backgroundColor: "{colors.chrome}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "2px 8px"
  room-switcher-hovered:
    backgroundColor: "{colors.hover}"
  room-switcher-opened:
    backgroundColor: "{colors.selected}"
  dialog:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.dialog}"
  alert-strip:
    backgroundColor: "{colors.danger-soft}"
    textColor: "{colors.text}"
    typography: "{typography.caption}"
    rounded: "{rounded.control}"
    padding: "4px 8px"
---

# Design System: Schwitzer

## Overview

**Creative North Star: "The Buddy List"**

The room is a buddy list. Tasks are contacts, each with a status flower; groups fold like ICQ groups; whoever works on a task wears a coloured name tag. The look is a modern, fresh ICQ: ICQ green on white by day, deep green-black by night, mint-tinted chrome, Figtree type, rounded 8px rows. It lives as a narrow window (400x780) beside the editor and terminal and is read in a glance, so everything is quiet, flat and tinted toward green; the only saturated things on screen are the flowers, the name tags and the green action fills.

The system is flat and calm. Depth is tonal (ground white, chrome mint, raised cards) with soft shadows only on things that float: dialogs, tooltips, the room-switcher menu. There is no motion by design: state changes are instant colour swaps, nothing animates, nothing blurs. Two themes, Daylight and Night, follow the system appearance (no preference means Night); every colour comes from the active theme's token set.

The signature is the stage-coloured eight-petal flower on every task row and in the status bar. It is the one status device: colour is the stage, outline-versus-filled marks the very first stage. It refuses the Kanban board and the wide three-pane admin layout.

**Key Characteristics:**
- Buddy-list structure: foldable groups of one-line rows, pick button at the right edge of finished plans.
- One saturated green for actions (`ICQ Green`), a separate leaf green for ink and focus, a separate running green for the flower.
- Every neutral leans mint-green; no cool or pure greys.
- Six member tag hues, fixed per member, identical for everyone; light fills with dark text by day, dark fills with light text by night.
- Figtree only, four weights (400 / 500 / 600 / 700), three sizes in practice (12 / 14 / 20).
- Flat rows, 1px hairlines, modal dialogs over a green-black scrim.

## Colors

A mint-and-green palette: white or green-black ground, mint chrome, one action green, and a small spectrum of stage and member colours that carry all the meaning. Hex values below are Daylight / Night; the frontmatter is normative (`night-` prefixed keys).

### Primary
- **ICQ Green** (#2db34a / Night #3dcb5c): the action fill. New task, Pick, Propose, Save, Accept, Approve, the selected row in a pick-list menu. Text on it is **Deep Green Ink** (#04210c / #03200b, `on-primary`).
- **ICQ Green, Hovered** (#27a142 / Night #5ad876): hover fill for primary buttons.
- **Leaf Ink** (#1a7f35 / Night #5ed77a): green that reads as text and as the focus ring: the pressed primary button, `#4` task links in sessions, the "Share mine" link, the active segment of the priority control, input focus border, notice text in the status bar.

### Secondary (stage flower colours)
Each is a flower tint; none is used as a surface.
- **New, Outline Grey** (#7d8c80 / #7d9182): Proposed. Drawn as the *outline* flower.
- **Idea, Faint Grey** (#a7b5aa / #5b6e61): an idea not yet written up into a proposal. Outline flower, fainter than New: an idea is less than a proposal. Idea and New are the only stages not filled.
- **Planning Amber** (#c98410 / #f0a630): Accepted and Planned without a ready plan; someone is writing or the plan is not final.
- **Pickable Blue** (#2f7fd6 / #5aa6f5): a plan is ready or approved and nobody builds it; the Pick button is available.
- **Running Green** (#22a340 / #3dcb5c): a claimed task, Accepted with a plan claim or Ready with a build claim; someone is working. Also the online dot, and the status-bar flower when the server is online.
- **Closed Muted** (#9aa89d / #5f7064): Done. Also the status-bar flower while connecting.
- **Rejected Red** (#d9473f / #f06a62): Rejected. Also the status-bar flower when the server is offline.
- **Alarm Red** (#c53a35 / #f06a62, `danger`): destructive buttons, high-priority arrow, offline dots, the alert strip border. Soft fill **Alarm Wash** (#fbe9e7 / #3a1a19, `danger-soft`); text on a pressed danger button **#ffffff / #2a0605**.

### Tertiary (member name tags)
Six hues, picked per member by a fixed hash, the same on every machine. Day: light fill + dark text of the same hue; Night: dark fill + light text.
- **Tag Green** fill #d8f3dc, text #14532d (Night #1d4a2a / #c6f6d0)
- **Tag Blue** fill #dbeafe, text #1e3a8a (Night #1e3a6b / #d3e4ff)
- **Tag Violet** fill #ede4fd, text #4c1d95 (Night #3b2a6b / #e6dcff)
- **Tag Orange** fill #ffe8cc, text #7c2d12 (Night #5a3416 / #ffe2c4)
- **Tag Pink** fill #fde2ef, text #831843 (Night #5c1f3d / #ffd9ea)
- **Tag Teal** fill #d5f2f0, text #134e4a (Night #16494a / #cdf5f2)

### Neutral
- **Paper White / Night Ground** (#ffffff / #121915, `ground`): the list and sessions ground.
- **Mint Chrome / Night Chrome** (#edf5ee / #0c110e, `chrome`): toolbar, status bar, pills, disabled inputs, the room switcher at rest. In Night the chrome is *darker* than the ground (the bars sink); in Daylight it is mint over white.
- **Raised** (#ffffff / #1a231e): dialogs, inputs, tooltips, menus, secondary buttons.
- **Mint Breath** (#f2f8f3 / #1b2620, `hover`): row or button under the pointer.
- **Mint Pressed** (#e2f1e5 / #213629, `selected`): pressed or selected row, text selection, active segment fill, opened room switcher.
- **Mint Hairline** (#d9e5db / #2a3a30, `line`): separators, resting input and secondary-button borders, scroller rails, disabled primary fill.
- **Sage Edge** (#8fa394 / #627568, `line-strong`): hovered input borders (3:1 on the ground), unknown-state dot, scroller while hovered.
- **Green-Black Ink / Night Text** (#142019 / #e4eee6, `text`): all primary text.
- **Moss Grey** (#56665a / #93a597, `muted`): secondary text, placeholders, inactive icons; documented in code as 4.5:1 on every surface.
- **Scrim** (rgba(12, 28, 17, 0.36) / rgba(0, 0, 0, 0.55)) and **Shadow** (rgba(12, 28, 17, 0.18) / rgba(0, 0, 0, 0.45)): the dim layer behind dialogs and the soft shadow of floating cards.

### Named Rules
**The Flower Carries Status Rule.** Stage is shown by the flower colour and nowhere else on a row: no row backgrounds, side stripes or coloured titles. A stage colour is never used for anything but a stage (and the server state mapped onto the same flower in the status bar).

**The Three Greens Rule.** Green means three different things and they do not swap: *ICQ Green* fills actions, *Leaf Ink* writes text and the focus ring, *Running Green* marks a running agent or a live connection. An action never takes the running hue and a flower never takes the action hue.

**The Mint Neutrals Rule.** Every neutral is tinted toward the green hue; chrome is mint, lines are mint, text is green-black. No pure grey, no blue-grey.

**The Colour Plus Form Rule.** Colour is never the only carrier. The flower sits next to a group title and, in the task dialog, next to a stage pill ("Plan ready", "In progress"); Idea and New are outline, not just grey. Name tags carry the member's name.

## Typography

**Display Font:** Figtree (bundled, no fallback needed; the app sets it as the default font)
**Body Font:** Figtree
**Label/Mono Font:** Figtree (no monospace; inline code in plans keeps the UI face)

**Character:** Figtree is a friendly geometric sans with slightly rounded terminals, the right register for a chat-era buddy list. Hierarchy comes from weight and from three sizes, not from many faces or tracking (iced has no letter spacing; all tracking is normal).

### Hierarchy
- **Headline** (700, 20px, 1.3): the task title in the task dialog.
- **Title** (600, 20px, 1.3): dialog titles ("New task", "Settings"); markdown `h1` in plans.
- **Heading** (600, 14px, 1.3): a block heading inside a dialog ("Plan", "Priority", "Notes", "Your name").
- **Body** (400, 14px, 1.3): task titles in rows, dialog text, inputs, session titles, plan text.
- **Body Medium** (500, 14px, 1.3): button labels and the `#n` number in the task dialog.
- **Label** (600, 12px, 1.3): group headers, "Sessions", member name tags, the Pick label, the status-bar user name.
- **Caption Medium** (500, 12px, 1.3): `#n` in rows, pills, session runtime, segment labels.
- **Caption** (400, 12px, 1.3, Moss Grey): counts, hints, project names, token counts, tooltips, empty-state hints.

Line height is iced's default relative 1.3 everywhere; no override.

### Named Rules
**The Three Sizes Rule.** 12 for captions, tags and counters; 14 for anything read as content; 20 for titles. Plan markdown steps `h2` 17 and `h3` 15 between them; nothing else invents a size.

**The Weight Does the Work Rule.** Emphasis is a weight step (400, 500, 600, 700), never a size jump, an underline or tracking.

## Layout

A single narrow column, designed for a 400x780 window (minimum 340x520), pinned beside the editor. From top to bottom: an optional dismissable error strip (8px inset), the **task list** (60% of the main area, `FillPortion(60)`), a 1px hairline, the **sessions** panel (38%, `FillPortion(38)`), a 1px hairline, the slim **button bar**, and the 32px **status bar**. New task, task view and settings are modal dialogs, never panes.

Spacing is a four-step scale: 4 / 8 / 12 / 16 (`sp-1`..`sp-4`). List rows sit 2px apart with 6px vertical and 8px horizontal inner padding; the list inset is 8px left, 12px right (keeps the Pick button clear of the scroll rail), 8px top and bottom. Dialog padding is 16px left/right/bottom, with a 4px-tighter header. Controls are 32px tall (text buttons, icon buttons at 16px glyph + 8px padding, status bar, sessions header).

The list is grouped in fixed order: **In progress** (every claimed task, planning or building), **Ready to pick**, **Planning**, **Proposals**, **Ideas**, **Done**. Empty groups disappear; groups fold with a chevron; within a group, higher priority first, then task number. Row content, left to right: flower (16px), `#n`, title (fills, wraps), priority arrow, the claim's work ("planning"/"building", muted) and member tag, Pick button. A wide window changes nothing except that dialogs grow to at most 560px wide.

Empty states are centred, max 280px: outline flower (40px, muted), a semibold title, a muted hint, one action.

Follows the system appearance: light = Daylight, dark (or unknown) = Night.

## Elevation & Depth

Hybrid but mostly flat: depth is tonal layering (Night chrome sinks below the ground, Daylight chrome tints mint above white; dialogs, inputs and menus use the `raised` tone) plus 1px hairlines. Rows, buttons, tags and bars have no shadow. Only floating surfaces cast one. There is no glass, blur or gradient.

### Shadow Vocabulary
- **Dialog** (`offset 0 8px, blur 28px, colour shadow`, with a 1px `line` border): the modal card above the scrim.
- **Menu** (`offset 0 4px, blur 16px, colour shadow`): the room-switcher menu.
- **Tooltip** (`offset 0 2px, blur 8px, colour shadow`): hover tips above the control, 4px gap.

### Named Rules
**The Flat-At-Rest Rule.** Anything inside the window is flat; only a surface that floats above it (dialog, menu, tooltip) may cast a shadow.

**The Scrim Rule.** A dialog dims everything behind it with the scrim colour; the base takes no input and a click on the scrim closes the dialog.

## Shapes

Soft but not bubbly. Everything interactive is a rounded rectangle with an 8px radius (`control`): rows, buttons, inputs, editors, the room switcher, menus, alert strip. Dialogs step up to 14px. Name tags, pills, dots and scroller rails are fully round (999px). Tooltips use 6px and inline code 4px (one-offs, see below). Borders are 1px hairlines in `line`; focus is 2px. Icons are Lucide (stroke, ISC) at 14px in rows and headers, 16px on buttons; the status flower is the authored `flower` / `flower-outline` SVG, 16px in rows, 14px in the status bar, 40px in empty states, tinted by theme tokens.

## Components

### Buttons
Four kinds plus a segmented control; every button is wrapped in an id container so tests and the headless control can address it.
- **Shape:** 8px radius; 32px tall (6px 12px padding around a 14px medium label; icon+label gap 6px, icon 16px).
- **Primary:** ICQ Green fill, Deep Green Ink text. Hovered: `primary-hover`. Pressed: Leaf Ink fill. Disabled: Hairline fill, Moss Grey text. One per view for the main call to action (New task, Propose, Save, Accept, Approve, "Pick and copy prompt").
- **Pick:** the compact primary on finished plans. Hand-grab icon (14px) + "Pick" in 12px semibold, padding 4px 10px, sits in the row's right edge, aligned with the tags of other rows. Disabled (muted icon) while a pick runs.
- **Secondary:** raised fill, Mint Hairline border; hovered Mint Breath with Sage Edge border; pressed Mint Pressed. Cancel, Replan, Reopen, Add, Save name, Release.
- **Danger:** red text and 1px red border on no fill; hovered Alarm Wash; pressed solid red with on-danger text. Reject.
- **Ghost:** no border, no fill; hover Mint Breath, pressed Mint Pressed. Icon buttons (32x32, 16px glyph, tooltip above) in toolbar and dialog headers, text links ("Share mine"), the `#n` session task link.
- **Segment:** 1px border; the active segment is Mint Pressed with Leaf Ink border and text (priority High / Normal / Low).

### List rows and group headers
- **Row:** full-width transparent button, 8px radius, hover Mint Breath, pressed Mint Pressed. Opens the task dialog.
- **Group header:** chevron (14px muted), title (12px semibold), muted count; folds the group. Same row button style, 4px padding.

### Name tags and pills
- **Name tag:** fully round, 2px 8px padding, 12px semibold; fill/text from the member's tag hue. Appears on rows with a claim, in sessions, in notes.
- **Pill:** quiet round label on Mint Chrome with a Hairline border and Moss Grey 12px medium text: "idle" on a session, the stage label in the task dialog.
- **Dot:** 8px round; running green up, red down, Sage Edge unknown. Server and MCP state in the status bar.

### Inputs / Fields
- **Style:** raised fill, 1px Mint Hairline border, 8px radius, 8px padding, 14px text, muted placeholder; text editors the same.
- **Hover:** border becomes Sage Edge.
- **Focus:** border becomes Leaf Ink at 2px (the focus ring); selection uses Mint Pressed.
- **Disabled:** Mint Chrome fill.
- **Enter** submits the field's action (propose, save, add note, save name).

### Navigation: toolbar, status bar, room switcher
- **Toolbar (slim button bar):** Mint Chrome, 4px 8px padding. Left: primary "New task" with plus, ghost "invite" icon (copy room key). Right: ghost settings icon with `Ctrl+,` tooltip. Actions that need a room are disabled and muted until one is selected.
- **Status bar:** 32px, Mint Chrome, padding 4px 8px. Left to right: status flower (14px; green online, red offline, muted connecting), my name (12px semibold), **room switcher**, spacer, server dot + "online/offline/connecting" (or a transient notice in Leaf Ink), MCP dot + "MCP". Both indicators have tooltips with detail.
- **Room switcher:** a pick list that is flat on the chrome until hovered (Mint Breath, Sage Edge border), opened = Mint Pressed with Leaf Ink border. Its menu is raised, hairline-bordered, 8px radius, selected entry in ICQ Green with on-primary text.

### Sessions
A 32px header ("Sessions", count, optional ghost "Share mine"), then rows: member tag, session title (14px), a caption line (project, `#n` task link in Leaf Ink, tokens, "idle" pill) and the runtime on the right. Rows are not buttons; they scroll in 8px inset.

### Dialogs
Modal card over the scrim: raised fill, 14px radius, 1px hairline, dialog shadow, max 560px wide, centred, 12px from the window edge. Header (title + 32px ghost close with `Esc` tooltip), scrolling body (16px side padding, 12px or 16px rhythm, hairline rules between blocks), pinned footer with right-aligned Cancel (secondary) + primary action. The tall dialog (settings) fills the height; the others (new task, task) hug their content. The task dialog stacks: flower + `#n` + stage pill, headline title, "Proposed by" tag, action row (primary, secondary, danger), Plan (markdown at 14px, links in Leaf Ink), Priority segments, Notes, add-note field.

### Scroll rails
Thin pill scrollers only, no track: Mint Hairline at rest, Sage Edge when hovered, Moss Grey while dragged.

### Alert strip
Alarm Wash fill, 1px red border, 8px radius; alert icon, 12px text, ghost dismiss. Sits above the list, 8px from the window edge.

### Motion
None, by design. Hover, press and focus are instant colour swaps; dialogs appear and disappear without transition; there are no animated indicators (the runtime counters tick once a second as text).

### Accessibility and input
iced 0.14 exposes no accessibility tree and screen readers cannot read the window, so contrast and keyboard operation carry accessibility. Muted text is held at 4.5:1 on every surface; input borders at hover reach 3:1 on the ground. Keyboard: `Tab` / `Shift+Tab` move focus through the fields, `Esc` closes the open dialog, `Ctrl+N` (Cmd on macOS) opens New task, `Ctrl+,` opens Settings, `Enter` submits the focused field. Focus is visible only on fields and editors (2px Leaf Ink ring): iced 0.14 buttons have no focused state, so buttons and rows are pointer targets (at least 32px tall) and every keyboard path must go through a shortcut or a field. Every icon-only button has a tooltip naming its action.

## Do's and Don'ts

### Do:
- **Do** express stage only with the flower colour from the six stage tokens (`stage-new` outline, `stage-planning`, `stage-pickable`, `stage-running`, `stage-closed`, `stage-rejected`).
- **Do** use `primary` for exactly the main action of a view and `ink` for green text, links and the focus ring.
- **Do** keep every neutral mint-tinted: chrome `#edf5ee` / `#0c110e`, lines `#d9e5db` / `#2a3a30`, text `#142019` / `#e4eee6`.
- **Do** read colours from the active theme's tokens through the style functions in `theme.rs`; add a new colour as a token pair (Daylight and Night) first.
- **Do** keep controls at 32px height and rows at 8px radius, 6px 8px padding, 2px apart.
- **Do** put a tooltip on every icon-only button and name the shortcut where one exists (`Settings  (Ctrl+,)`, `Close  (Esc)`).
- **Do** keep dialogs modal with the scrim, max 560px wide, footer pinned with Cancel then the primary action.
- **Do** write weight, not size, to emphasise: 400 / 500 / 600 / 700 in 12 / 14 / 20.

### Don't:
- **Don't** recolour rows, tint backgrounds or add side stripes to show status; the flower is the status.
- **Don't** introduce a Kanban board, a wide multi-pane layout or any pane beside the list; new surfaces are dialogs.
- **Don't** use ICQ Green for decoration, or Running Green for a button.
- **Don't** add shadows to anything that does not float; no gradients, blur or glass.
- **Don't** add animation or transitions; the product is motionless by design.
- **Don't** use pure grey, pure black or a cool grey; Night ground is green-black `#121915`, not `#000000`.
- **Don't** use a second typeface, tracking, or sizes beyond 12 / 14 / 20 (plus plan headings 15 / 17).
- **Don't** rely on colour alone: a stage needs the flower form (outline for New) and a label; a person needs a named tag.
- **Don't** assume buttons are keyboard focusable in iced 0.14; every action reachable only by button needs a shortcut or an obvious pointer path.

### Not canonized (defects the build carries)
- Layout literals outside the token scale: 6px icon-label gaps and row padding, 10px Pick button padding, 2px list gaps, 1px/2px tag padding, tooltip radius 6px, inline-code radius 4px, plan heading sizes 15 / 17. They are one-offs of the build, kept above for fidelity but not extended to new surfaces.
- No visible focus indicator on buttons, rows and the room switcher (a limit of iced 0.14's button status).
