---
name: iced-impeccable
description: Use only when the project is an iced (Rust) desktop app and the task is designing, redesigning, critiquing, auditing, polishing or otherwise improving its UI, or visually verifying it headless (start it offscreen, inject keyboard and mouse input, take screenshots). Load it alongside the impeccable skill: impeccable owns the turn (commands, doctrine, modes); this skill layers iced-native guidance and headless verification on top. Not for web, iOS or Android UI.
version: 0.3.0
user-invocable: true
argument-hint: "[impeccable command] [target] | verify | integrate"
license: MIT OR Apache-2.0
---

This skill layers iced (Rust, 0.14) desktop guidance on top of the `impeccable` skill. The design doctrine stays impeccable's; the platform (widgets, layout, input, verification) is iced's.

## Relationship

Read the impeccable skill's `SKILL.md` first (`.claude/skills/impeccable` or `.omp/skills/impeccable` in the project, or wherever the harness installed it). Its principles, modes, commands and references apply, except where this skill overrides them. An iced app surface is **Operate** mode unless the brief says otherwise: scanability, consistency and desktop expectations outrank expression, and brand lives in precise details.

Impeccable's bounded-pass rule holds unchanged: build fully, inspect once with a batched capture round, fix everything in one batch, confirm at most once, stop.

## Setup

1. Run impeccable's `context` launcher exactly as its Setup says (`"<impeccable skill dir>/scripts/impeccable" context`, once per session, cwd at the user's project).
2. PRODUCT.md `## Platform` is `desktop` and `## Stack` records `iced 0.14 (Rust)`. Impeccable (checked against 4.7.0) recognises only `web`, `ios`, `android` and `adaptive`, so the launcher treats `desktop` as unrecognized (web) and resolves `"platform": null`. Expected: do not "fix" the field and do not surface it to the user. Handle the launcher's directives like this:
   - `MANUAL_DETECTOR_REQUIRED`: web-only; ignore it. Never run `impeccable detect` on an iced app.
   - `IMAGE_TOOLS`: ignore it for screenshots; crops, zooms and comparisons go through `screenshot --crop X Y W H --zoom N` and `iced-impeccable sheet` ([reference/verify.md](reference/verify.md)), never `magick` or `ffmpeg`.
   - `WORLD_DISCOVERY_REQUIRED`: the launcher does not recognise Rust views as a visual implementation (`"hasVisualImplementation": false`). An app with existing iced views has an incumbent implementation; the directive applies only to a new build or an explicit redesign, as it says.
   - `AUTONOMY_DIRECTIVE_CHECK`, `SUBAGENT_AUTHORIZATION` and any other platform-neutral directive (`CONTEXT_STALE`, `UPDATE_AVAILABLE`, `IMAGE_GEN_AVAILABLE`): follow as impeccable says.
3. Read [reference/iced.md](reference/iced.md).
4. Before any UI edit, read impeccable's `craft-floor.md`, then the "Craft floor on iced" section of [reference/iced.md](reference/iced.md), which translates it.
5. During impeccable `init`, write Platform `desktop` and Stack `iced 0.14 (Rust)`, and skip its live-mode configuration step.

If a later impeccable recognises `desktop` natively, Setup and the native-equivalence rule below are the only places to change.

## Native equivalence

With `"platform": null` none of impeccable's native branches fire by themselves. Apply them by hand: every impeccable branch written for `ios` / `android` / `adaptive` applies to iced. [reference/iced.md](reference/iced.md) stands in for `ios.md` / `android.md`, and [reference/verify.md](reference/verify.md) for the platform reference's "Verifying the build". Its capture set replaces `desktop.png` / `mobile.png` and the per-device-class captures. No detector runs, ever.

- `critique`: the native path (see the table).
- `animate`: the Motion section of [reference/iced.md](reference/iced.md), including reduced motion.
- `polish`: representative sizes and QA become the verify.md capture set, plus the `wait-idle` replies and the host log.
- New work (impeccable's `new-work.md`, Verify and finish): code-led. Run the capture round into `.impeccable/review/`, one file per capture; skip the detector step; the finish reviewer gets `reference/iced.md`, the capture list and "no detector ran; platform is iced desktop".
- `optimize`: see the table.

## Command overrides

| Command | Use |
|---|---|
| `audit` | [reference/audit.iced.md](reference/audit.iced.md) instead of impeccable's `audit.md` |
| `adapt` | [reference/adapt.iced.md](reference/adapt.iced.md) instead of impeccable's `adapt.md` |
| `live`, `generate` | [reference/variants.iced.md](reference/variants.iced.md) (contact-sheet variants, no browser picker) |
| `document`, `extract` | impeccable's `document.md` / `extract.md` plus [reference/design-md.iced.md](reference/design-md.iced.md) |
| `hooks`, `detect` | Not available on iced. Say so in one line and move on. |
| `critique` | Impeccable's `critique.md`, native path: Assessment A alone. No detector, no browser overlay, no sub-agent gates, no degraded banner. Header `Method: design review only (detector not applicable, iced desktop)`. Deterministic scan: `not applicable (iced desktop)`. Inspect with the capture set in [reference/verify.md](reference/verify.md). Persistence unchanged. |
| `optimize` | Impeccable's `optimize.md` is web-only. Diagnose with the Performance checks of [reference/audit.iced.md](reference/audit.iced.md), fix in Rust, measure with `wait-idle` / `tree` before and after. |
| `craft`, `shape`, comp-led builds | `craft` is an alias of ordinary new work, code-led. `comp-spec`, `region-map`, `component-review`, hero and plate gates, `build-phase` and image-generated comps do not apply; `buildPath` / `config-build-path-*` findings are not applicable. |
| `doctor` | Run `doctor --json`. A finding about `## Platform` `desktop` is expected: report it in one line and never repair it. Run `doctor --fix` only when no `auto` finding touches PRODUCT.md `## Platform`. |
| `/impeccable` with no argument | Impeccable's `routing.md` web guards do not fire with `platform: null`, so skip them by hand: no `impeccable detect` on `scan.targets`, no `live` / `generate` as browser modes. Offer `verify` and the contact-sheet variants instead. Never auto-run. |
| every other command | impeccable's own reference; translate its web examples (CSS, HTML, media queries) with [reference/iced.md](reference/iced.md) |

## Verification

Every visual check goes through [reference/verify.md](reference/verify.md): a headless instance of the app, driven through its control socket, screenshotted with `iced-impeccable ctl`. Never the user's desktop, never a browser, never external image tools.

`verify` as the argument runs that flow on the current app and reports what it saw.

## Integrate

Run when the user passes `integrate`, and whenever the target app does not yet support `--headless --control`. Check by searching the whole workspace, member crates included, for `iced_impeccable` in every `Cargo.toml` and in source (`Remote::new`); then read README and AGENTS/CLAUDE.md for a documented headless script and use it. To put this skill into the repository, run `iced-impeccable install [--agent claude,omp]` (add `--yes` to skip prompts); it copies the skill into the repo's agent skill dirs after removing a marketplace install of the plugin.

1. `Cargo.toml` of the app:
   ```toml
   [dependencies]
   iced_impeccable = { version = "0.3", optional = true, default-features = false, features = ["wgpu", "tiny-skia"] }

   [features]
   remote = ["dep:iced_impeccable"]
   ```
   When a local checkout of iced_impeccable exists next to the app's repository, a `path` dependency on it works the same and picks up unreleased changes. The `iced-impeccable` binary comes from `cargo install iced_impeccable --bin iced-impeccable --locked`.
2. Wrap `main`: build the `iced::application(...)` or `iced::daemon(...)` value, then
   ```rust
   #[cfg(feature = "remote")]
   return iced_impeccable::Remote::new(app)
       .heartbeat(|m| matches!(m, Message::Tick(_) | Message::Frame(_)))
       .run();
   #[cfg(not(feature = "remote"))]
   app.run()
   ```
3. Mark periodic subscription messages (`iced::time::every`, `iced::window::frames`) as heartbeats in the closure above, so `wait-idle` is not kept busy by them.
4. Give icon-only interactive widgets an id: `container(button(icon).on_press(..)).id("settings")`, then address them with `tap #settings`. Pick-list entries and other widgets that report no text are reached the same way or via `tree`.
5. App code that parses argv uses `iced_impeccable::app_args()` (argv without the host flags). App code that must not open OS dialogs (file pickers, message boxes) checks `iced_impeccable::is_headless()` and falls back. Tray icons and other desktop integration go into `.windowed(..)`; headless-only setup into `.headless(..)`.
6. App-specific actions the agent needs (reset state, restart, seed data) are registered with `.command("name", "summary", |args| Ok(Message::...))`.
7. Release builds never enable `remote`.

After integrating, verify with [reference/verify.md](reference/verify.md).
