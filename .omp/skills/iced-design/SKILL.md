---
name: iced-design
description: Use only when the project is an iced (Rust) desktop app and the task is designing, redesigning, critiquing, auditing, polishing or otherwise improving its UI, or visually verifying it headless (start it offscreen, inject keyboard and mouse input, take screenshots). Works alone or under another design skill such as impeccable: that skill keeps its doctrine and commands, this one supplies iced-native guidance and headless verification. Not for web, iOS or Android UI.
version: 0.4.0
user-invocable: true
argument-hint: "verify | audit | adapt | variants [n] [element] | integrate | [design-skill command] [target]"
license: MIT OR Apache-2.0
---

Guidance for iced 0.14 (Rust) desktop UI (widgets, layout, input, theming) plus headless verification of the running app through `iced-remote`.

## Other design skills

If the impeccable skill is available (loaded in this session, or `.claude/skills/impeccable/` or `.omp/skills/impeccable/` exists in the project), read [reference/impeccable.md](reference/impeccable.md) first and follow it. It decides how impeccable's commands run on iced.

When any other design skill is active:

- Its design doctrine leads.
- Translate its web examples (CSS, HTML, media queries) with [reference/iced.md](reference/iced.md).
- Replace every browser, dev-server, Playwright or screenshot step with [reference/verify.md](reference/verify.md).
- Never run its web tooling (linters, detectors, live pickers) on an iced app.
- Where it builds variants in a browser, use [reference/variants.md](reference/variants.md).

One skill leads a turn: the one whose command the user invoked. The others contribute rules only.

## Working alone

Read [reference/iced.md](reference/iced.md) before any UI edit. Work in bounded passes: build fully, inspect once with the capture set of [reference/verify.md](reference/verify.md), fix everything in one batch, confirm at most once, stop.

|Argument|Use|
|---|---|
|`verify`|the [reference/verify.md](reference/verify.md) flow on the current app, report what it saw|
|`audit`|[reference/audit.md](reference/audit.md)|
|`adapt`|[reference/adapt.md](reference/adapt.md)|
|`variants [n] [element]`|[reference/variants.md](reference/variants.md)|
|`integrate`|Integrate below|
|none|ask what to do and offer verify, audit and variants; never auto-run|

## Verification

Every visual check goes through [reference/verify.md](reference/verify.md): a headless instance of the app, driven through its control socket, screenshotted with `iced-remote ctl`. Never the user's desktop, never a browser, never external image tools.

`verify` as the argument runs that flow on the current app and reports what it saw.

## Integrate

Run when the user passes `integrate`, and whenever the target app does not yet support `--headless --control`. Check by searching the whole workspace, member crates included, for `iced_remote` in every `Cargo.toml` and in source (`Remote::new`); then read README and AGENTS/CLAUDE.md for a documented headless script and use it. To put this skill into the repository, run `iced-remote install [--agent claude,omp]` (add `--yes` to skip prompts); it copies the skill into the repo's agent skill dirs, removing a marketplace or linked install of the plugin and an old `iced-impeccable` copy.

1. `Cargo.toml` of the app:
   ```toml
   [dependencies]
   iced_remote = { version = "0.4", optional = true, default-features = false, features = ["wgpu", "tiny-skia"] }

   [features]
   remote = ["dep:iced_remote"]
   ```
   When a local checkout of iced_remote exists next to the app's repository, a `path` dependency on it works the same and picks up unreleased changes. The `iced-remote` binary comes from `cargo install iced_remote --bin iced-remote --locked`.
2. Wrap `main`: build the `iced::application(...)` or `iced::daemon(...)` value, then
   ```rust
   #[cfg(feature = "remote")]
   return iced_remote::Remote::new(app)
       .heartbeat(|m| matches!(m, Message::Tick(_) | Message::Frame(_)))
       .run();
   #[cfg(not(feature = "remote"))]
   app.run()
   ```
3. Mark periodic subscription messages (`iced::time::every`, `iced::window::frames`) as heartbeats in the closure above, so `wait-idle` is not kept busy by them.
4. Give icon-only interactive widgets an id: `container(button(icon).on_press(..)).id("settings")`, then address them with `tap #settings`. Pick-list entries and other widgets that report no text are reached the same way or via `tree`.
5. App code that parses argv uses `iced_remote::app_args()` (argv without the host flags). App code that must not open OS dialogs (file pickers, message boxes) checks `iced_remote::is_headless()` and falls back. Tray icons and other desktop integration go into `.windowed(..)`; headless-only setup into `.headless(..)`.
6. App-specific actions the agent needs (reset state, restart, seed data) are registered with `.command("name", "summary", |args| Ok(Message::...))`.
7. Release builds never enable `remote`.

After integrating, verify with [reference/verify.md](reference/verify.md).
