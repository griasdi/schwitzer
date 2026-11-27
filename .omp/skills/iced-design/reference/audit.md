Run systematic **technical** quality checks on an iced desktop app and generate a comprehensive report. Don't fix issues; document them for other commands to address.

This is a code-level audit plus a headless check, not a design critique. Audit from the Rust source (views, style functions, subscriptions, theme module) and confirm behaviour on a headless instance per [verify.md](verify.md). No browser tooling applies. Score against [iced.md](iced.md), including its slop test; read it first if you have not yet. The report format is under Report below; under impeccable use [impeccable.md](impeccable.md) "Audit report" instead.

## Diagnostic Scan

Run comprehensive checks across 5 dimensions. Score each dimension 0-4 using the criteria below.

### 1. Accessibility

iced 0.14 exposes no accessibility tree, so screen-reader support is out of scope; say so once in the report and score what the app controls.

**Check for**:
- **Keyboard reachability**: start headless, then loop `key tab` + `focused` through the window (`focused` replies `ok X Y W H` or `err none`). Every focusable widget must be visited; count what was reached against `tree` nodes of kind `input` / `focusable`. iced 0.14 buttons, checkboxes, radios, togglers, sliders and pick lists take no keyboard focus (a button reports as `container`), so check that every such action also has a keyboard path (shortcut, Enter on the form's input, a custom focusable widget). Unreachable actions, focus traps and focus lost after navigation are findings.
- **Visible focus**: `screenshot --crop X Y W H --zoom 3` on the focused widget in both appearances; a focused control that looks like an unfocused one fails.
- **Contrast**: text below 4.5:1 (large text 3:1), UI boundaries below 3:1, in dark and light (`appearance light`); disabled states included.
- **Scale**: layouts that clip or overlap at `scale 1.5` and `scale 2`; text under 12 logical px.
- **Pointer targets**: clickable controls under 24x24 logical (32x32 for primary actions); read the `box` sizes from `tree`.
- **Colour-only meaning** and missing text alternatives for icon-only buttons (no tooltip, no id).

**Score 0-4**: 0=Keyboard unusable, 1=Major gaps (controls unreachable, no focus indication), 2=Partial (reachable, but focus or contrast breaks), 3=Good (minor gaps), 4=Excellent (fully keyboard-operable, visible focus, contrast and scaling clean)

### 2. Performance

**Check for**:
- **Perpetual redraw at rest**: `wait-idle` on an idle screen replying `ok animating` means something requests frames forever (a `window::frames()` subscription that never drops, a `time::every` with a short period, an `Animation` never finishing). `err timeout` means the UI keeps changing.
- **Heavy work in `view`**: allocation-heavy list building, sorting, formatting, or parsing on every call; `view` runs on every update.
- **Missing caching**: custom drawing without `canvas::Cache` (cache cleared only on real change); expensive subtrees that could use `iced::widget::lazy` (needs iced's `lazy` feature) with a cheap dependency key.
- **Image handles rebuilt per view**: `image::Handle::from_bytes(..)` or `svg::Handle` created inside `view` instead of stored in state (each new handle defeats the GPU cache).
- **Blocking work in `update`**: file, network or CPU-heavy work run synchronously instead of in a `Task::perform` / `Task::future`; the UI freezes (visible as `wait-idle` and `tap` replies stalling).
- **Unvirtualized long lists**: thousands of rows in one `column!` inside a `scrollable`; page, window or virtualise them. Check the `Row 1..N` style hidden rows in `tree`: node count grows with content.
- **Startup**: heavy work before the first frame (`boot` / `new`) instead of a `Task` after it.

**Score 0-4**: 0=Janky or frozen, 1=Major problems (perpetual redraw, blocking `update`), 2=Partial, 3=Good (minor improvements possible), 4=Excellent (idle at rest, cached, async I/O, lean)

### 3. Appearance & Theming

**Check for**:
- **Raw colours outside the theme module**: `Color::from_rgb(..)`, `color!(..)`, hex literals in views and style functions instead of `theme.extended_palette()`.
- **Single appearance**: a pinned `Theme::Dark` with no light variant (or the reverse) when the product does not pin one; compare `desktop-standard-dark.png` and `desktop-standard-light.png`.
- **Stock palette**: unmodified `Theme::Dark` / `Theme::Light` / built-in themes with no brand decision (no `Theme::custom`).
- **Inconsistent style functions**: several near-identical button/container styles; style functions that ignore `Status`; radii, borders and shadows set inline instead of from tokens.
- **Token drift**: spacing, size and radius literals instead of the consts; fonts not bundled (`.font(..)`, `default_font`), so headless and other OSes differ.
- **Off-toolkit effects**: faked glass or blur, hard offset shadows, gradient text, side-stripe accents.

**Score 0-4**: 0=Hard-coded everything, 1=Minimal tokens, 2=Partial (theme exists, inconsistently used), 3=Good (minor raw values), 4=Excellent (palette-driven throughout, both appearances first-class)

### 4. Desktop Conformance (CRITICAL)

Score against [iced.md](iced.md), including its slop test. **Check for**:
- **Resize**: the window is resizable and `min_size` is set; `resize 960 600` and `resize 1920 1080` leave nothing clipped, overlapped or floating in empty space.
- **Shortcuts**: frequent actions have keyboard shortcuts, shown in tooltips; `Modifiers::command()` used so macOS gets Cmd.
- **Tab order**: `focus_next` / `focus_previous` wired; order matches reading order.
- **Enter and Escape**: forms submit on Enter (`on_submit`); Escape closes overlays and modals, never quits.
- **Right click**: context actions on rows and text via `mouse_area(..).on_right_press`.
- **Clipboard**: copy/paste work (`clip`, `clip-set` round-trip); selection-friendly text.
- **HiDPI**: crisp and unclipped at scale 2 (`desktop-hidpi-dark.png`); no pixel-snapped assumptions.
- **Close request with unsaved state**: the app handles `window::close_requests()` / `Event::Window(CloseRequested)` and guards data loss; `quit` replying `ok forced` after 5 s shows the app ignores or stalls on close.
- **Native file dialogs**: file open/save use the platform dialog (for example `rfd`) behind `is_headless()`; a missing dialog fallback blocks headless runs.
- **Web-shaped patterns**: hamburger menus, bottom tab bars, hover-only affordances, emoji icons, mixed icon sets.

**Score 0-4**: 0=Web port (nothing desktop), 1=Heavy violations (3-4 kinds), 2=Some (1-2 noticeable), 3=Mostly conformant (subtle issues), 4=Fully desktop-native in behaviour (a keyboard-fluent user trusts every screen)

### 5. Adaptivity

**Check for**:
- **Size classes**: compact 960x600, standard 1280x800, wide 1920x1080 each have a deliberate layout (`responsive`); the wide class is not a stretched standard layout, the compact class does not hide core functions.
- **Scale factors** 1 / 1.5 / 2 render without clipping or overlap.
- **Long and translated strings**: text 40% longer than English still wraps and keeps controls reachable; no truncation that hides meaning.
- **Fixed-pixel layouts** (`Length::Fixed` for content regions) that break on resize.
- **Pane behaviour**: `pane_grid` panes with sane `min_size`; sidebars collapse on compact.
- **Content extremes**: empty lists, one item, thousands of items, very long names.

**Score 0-4**: 0=One window size only, 1=Major breakage (compact or wide broken), 2=Partial, 3=Good (minor edge cases), 4=Excellent (adapts across size classes, scale factors and string lengths)

## Report

The report lives in the chat; write it to a file only when the user asks. Sections in this order:

### Desktop Conformance verdict

First in the report. Pass or fail: does the app read as a desktop app, or as a ported website or the stock iced example? Name the captures and `ctl` commands that are the evidence.

### Health score

| # | Dimension | Score | Key finding |
|---|---|---|---|
| 1 | Accessibility | 0-4 | |
| 2 | Performance | 0-4 | |
| 3 | Appearance & Theming | 0-4 | |
| 4 | Desktop Conformance | 0-4 | |
| 5 | Adaptivity | 0-4 | |
| | **Total** | **??/20** | band |

Bands: 18-20 Excellent, 14-17 Good, 10-13 Acceptable, 6-9 Poor, 0-5 Critical.

### Summary

The score and band, issue counts per severity, the top 3-5 issues, next steps.

### Findings

Severity:
- **P0 blocking**: a task cannot be completed.
- **P1 major**: serious difficulty or a broken [iced.md](iced.md) rule; fix before release.
- **P2 minor**: an annoyance with a workaround.
- **P3 polish**: keep these few.

Each finding, P0 first:

```
[P1] Short name
Location: view, file, line
Category: one of the five dimensions
Impact: who is affected and how
Rule: the iced.md section it breaks
Fix: the concrete change
```

### Patterns

Systemic issues, for example: "Raw `Color::from_rgb` appears in 15+ views, should use `extended_palette()`"; "Icon-only buttons are below 24x24 and carry no tooltip or id throughout the toolbar".

### Positives

What works and should stay.

### Next steps

Ordered from P0 down, each naming a skill argument (`adapt`, `variants`, `verify`) or the concrete code change.

Screen-reader support is out of scope (iced 0.14 has no accessibility tree): say so once and score what the app controls.

**Avoid**:
- Driving the user's desktop or a browser to gather evidence
- Scoring screen-reader support the toolkit cannot provide
- Findings without their impact
- Unverified findings
