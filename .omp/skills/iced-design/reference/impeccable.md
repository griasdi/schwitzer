# iced-design under impeccable

When the impeccable skill is active, impeccable owns the turn (commands, doctrine, modes); this file maps it onto iced. Checked against impeccable 4.9.1. When impeccable changes, only this file and [impeccable-design-md.md](impeccable-design-md.md) change.

## Relationship

Read the impeccable skill's `SKILL.md` first (`.claude/skills/impeccable` or `.omp/skills/impeccable` in the project, or wherever the harness installed it). Its principles, modes, commands and references apply, except where this skill overrides them. An iced app surface is **Operate** mode unless the brief says otherwise: scanability, consistency and desktop expectations outrank expression, and brand lives in precise details.

Impeccable's bounded-pass rule holds unchanged: build fully, inspect once with a batched capture round, fix everything in one batch, confirm at most once, stop.

## Setup

1. Run impeccable's `context` launcher exactly as its Setup says (`"<impeccable skill dir>/scripts/impeccable" context`, once per session, cwd at the user's project).
2. PRODUCT.md `## Platform` is `desktop` and `## Stack` records `iced 0.14 (Rust)`. Impeccable (checked against 4.9.1) recognises only `web`, `ios`, `android` and `adaptive`, so the launcher treats `desktop` as unrecognized (web), prints a `WARNING` asking to fix the field, and resolves `"platform": null`. Expected: do not "fix" the field and do not surface it to the user. Handle the launcher's directives like this:
   - `MANUAL_DETECTOR_REQUIRED`: web-only; ignore it. Never run `impeccable detect` on an iced app.
   - `IMAGE_TOOLS`: ignore it for screenshots; crops, zooms and comparisons go through `screenshot --crop X Y W H --zoom N` and `iced-remote sheet` ([verify.md](verify.md)), never `magick` or `ffmpeg`.
   - `WORLD_DISCOVERY_REQUIRED`: the launcher does not recognise Rust views as a visual implementation (`"hasVisualImplementation": false`). An app with existing iced views has an incumbent implementation; the directive applies only to a new build or an explicit redesign, as it says.
   - `AUTONOMY_DIRECTIVE_CHECK`, `SUBAGENT_AUTHORIZATION` and any other platform-neutral directive (`CONTEXT_STALE`, `UPDATE_AVAILABLE`, `IMAGE_GEN_AVAILABLE`): follow as impeccable says.
3. Read [iced.md](iced.md).
4. Before any UI edit, read impeccable's `craft-floor.md`, then the "Craft floor" section of [iced.md](iced.md), which translates it.
5. During impeccable `init`, write Platform `desktop` and Stack `iced 0.14 (Rust)`, and skip its live-mode configuration step.

If a later impeccable recognises `desktop` natively, Setup and the native-equivalence rule below are the only places to change.

## Native equivalence

With `"platform": null` none of impeccable's native branches fire by themselves. Apply them by hand: every impeccable branch written for `ios` / `android` / `adaptive` applies to iced. [iced.md](iced.md) stands in for `ios.md` / `android.md`, and [verify.md](verify.md) for the platform reference's "Verifying the build". Its capture set replaces `desktop.png` / `mobile.png` and the per-device-class captures. No detector runs, ever.

- `critique`: the native path (see the table).
- `animate`: the Motion section of [iced.md](iced.md), including reduced motion.
- `polish`: representative sizes and QA become the verify.md capture set, plus the `wait-idle` replies and the host log.
- New work (impeccable's `new-work.md`, Verify and finish): code-led. Run the capture round into `.impeccable/review/`, one file per capture; skip the detector step; the finish reviewer gets `iced.md`, the capture list and "no detector ran; platform is iced desktop" (see "Captures and handoff" below).
- `optimize`: see the table.

## Command overrides

| Command | Use |
|---|---|
| `audit` | [audit.md](audit.md) instead of impeccable's `audit.md`; report as in "Audit report" below |
| `adapt` | [adapt.md](adapt.md) instead of impeccable's `adapt.md`, together with impeccable's `adapt.native.md` principles |
| `live`, `generate` | [variants.md](variants.md) (contact-sheet variants, no browser picker), with the question asked through impeccable's question tool |
| `document`, `extract` | impeccable's `document.md` / `extract.md` plus [impeccable-design-md.md](impeccable-design-md.md) |
| `hooks`, `detect` | Not available on iced. Say so in one line and move on. |
| `critique` | Impeccable's `critique.md`, native path: Assessment A alone. No detector, no browser overlay, no sub-agent gates, no degraded banner. Header `Method: design review only (detector not applicable, iced desktop)`. Deterministic scan: `not applicable (iced desktop)`. Inspect with the capture set in [verify.md](verify.md). Persistence unchanged. |
| `optimize` | Impeccable's `optimize.md` is web-only. Diagnose with the Performance checks of [audit.md](audit.md), fix in Rust, measure with `wait-idle` / `tree` before and after. |
| `craft`, `shape`, comp-led builds | `craft` is an alias of ordinary new work, code-led. `comp-spec`, `region-map`, `component-review`, hero and plate gates, `build-phase` and image-generated comps do not apply; `buildPath` / `config-build-path-*` findings are not applicable. |
| `doctor` | Run `doctor --json`. A finding about `## Platform` `desktop` is expected: report it in one line and never repair it. Run `doctor --fix` only when no `auto` finding touches PRODUCT.md `## Platform`. |
| `foundations` | Run it as impeccable's `foundations.md` says; web items (`not-found-page`, `app-icons`, `social-previews`, `legal-pages`, `error-tracking`) do not apply to an iced app. `readable-contrast`, `no-generic-ai-patterns`, `design-tokens` and `reduced-motion` stay `not checked` because the detector does not read Rust: never run `impeccable detect` for them; judge them by hand against [iced.md](iced.md) (Motion for reduced motion) and the Accessibility and Appearance & Theming checks of [audit.md](audit.md). |
| `/impeccable` with no argument | Impeccable's `routing.md` web guards do not fire with `platform: null`, so skip them by hand: no `impeccable detect` on `scan.targets`, no `live` / `generate` as browser modes. Offer `verify` and the contact-sheet variants instead. Never auto-run. |
| every other command | impeccable's own reference; translate its web examples (CSS, HTML, media queries) with [iced.md](iced.md) |

## Captures and handoff

Under impeccable the capture set of [verify.md](verify.md) goes to `.impeccable/review/` instead of `target/ui-review/`, with the same file names; the `mkdir` and `sheet` paths change the same way.

After the round, pass the path to [iced.md](iced.md), the capture file names as the required window classes, and "no detector ran; platform is iced desktop" to `impeccable-finish-reviewer`.

## Audit report

Generate the report exactly as `audit.native.md` "Generate Report" says (health score table and rating bands, verdict, executive summary, findings by P0-P3 severity, patterns, positives, recommended actions, closing message), with these overrides:

- **Desktop Conformance** replaces "Platform Conformance" (row 4 of the score table and the verdict heading, "Desktop Conformance Verdict"). The verdict asks whether the app reads as a desktop app or a ported website / the stock iced example, and names the captures and `ctl` commands that are the evidence.
- **Guideline** cites the [iced.md](iced.md) rule; **Location** is view, file, line.
- Pattern examples: "Raw `Color::from_rgb` appears in 15+ views, should use `extended_palette()`"; "Icon-only buttons are below 24x24 and carry no tooltip or id throughout the toolbar".
- Screen-reader support is out of scope (iced 0.14 has no accessibility tree): say so once and score what the app controls.

After delivering it, save it as `audit.native.md` "Save the Report" says (`audit-storage write`).

**Avoid**, in addition to `audit.native.md`'s list:
- Driving the user's desktop or a browser to gather evidence
- Running `impeccable detect` or hooks (not available on iced)
- Scoring screen-reader support the toolkit cannot provide
