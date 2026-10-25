# AGENTS.md

Schwitzer is a task room for coding agents and their humans: a desktop app in Rust and iced (Linux, Windows, macOS) with a local MCP server for the agents, and a standalone room server over QUIC (weida). PRODUCT.md says what it is for, DESIGN.md how it looks, docs/ARCHITECTURE.md how it fits together.

## Commands

```sh
cargo build --workspace --features schwitzer-app/remote   # server + app with headless remote control
target/debug/schwitzer-server --listen 127.0.0.1:7454 --data-dir /tmp/sw/server   # prints `server url: weida://sha256:…@host:port` on stdout; also --public-host
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
deploy/release.sh 0.2.0   # on main, pushed: set the version, tag v0.2.0 (.github/workflows/release.yml builds and publishes the desktop app and the Linux server; the same workflow publishes every commit on main that passed CI as the rolling `dev` prerelease)
deploy/changelog.sh v0.1.0..HEAD   # release notes from the Changelog: trailers
cargo run -p schwitzer-app --example licenses   # rewrite assets/licenses/third-party.json after Cargo.lock changed (needs cargo-about 0.9.2)
```

Never `cargo run -p schwitzer-app` or start `schwitzer` without `--headless`: see "Running headless". There is no xtask. GitHub Actions (`.github/workflows/ci.yml`) runs the gate on every push to `main` and every pull request, and app clippy and the `update::` tests on macOS and Windows; the local gate below still runs before every push.

## Gate

Every change that touches code passes the gate before it is committed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features schwitzer-app/remote -- -D warnings
cargo test --workspace
cargo deny check
```

stkn.org runs the latest release and installs it itself (`schwitzer-server --auto-update`, release channel); no agent deploys, restarts or replaces it (see "Finishing a change").

`cargo deny check` (`deny.toml`) fetches the RustSec advisory database and checks the graph with every feature: it fails on every vulnerability, on an "unmaintained" notice only for a direct dependency, on a license outside the allow list, and on a source other than crates.io and the two git repositories listed there; a crate in two versions is a warning. An advisory is never silenced, a license never allowed and a source never added without the user's decision. Install it with `cargo install --locked cargo-deny`.

A change to doc comments also runs `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`. A UI change is checked the way a user meets it: clicked through headless and screenshotted, not only compiled. `crates/schwitzer-client/tests/flow.rs` starts a real server on loopback UDP and restarts it on the same data dir; it needs no network beyond `127.0.0.1`.

## Finishing a change

A change is finished when its gate passed and it is committed and pushed to `origin main`. A change without code, such as documentation only, skips the gate but is still committed and pushed. A change does not deploy stkn.org: the server there installs each published release itself, so a protocol or schema change reaches the people using it together with the app that speaks it.

- Stage paths explicitly, never `git add -A` or `git add .`: the checkout may hold the user's uncommitted work, which is left alone. `SCHWITZER_PLAN.md` and other local planning files are never committed; `.omp/mcp.json` holds a token and stays untracked.
- Commit messages follow CONTRIBUTING.md: no prefix, a `Changelog:` trailer for user-visible changes.
- Before pushing, `git pull --rebase origin main`; if that brought new commits, run the gate again. Push only to `origin`.
- stkn.org is the shared server (`weida://…@stkn.org:7444`), which other people use. It runs `schwitzer-server --auto-update` (release channel) as the systemd user service `schwitzer-server` and installs a published release within two minutes. Agents never deploy, restart or replace it and never run `deploy/deploy.sh`, the user's installer for that service. A change that breaks the protocol or migrates the schema cuts off older apps once the next release is published, until they update, so say so in the report. Logs: `ssh stkn.org journalctl --user -u schwitzer-server`.
- stkn.org also runs the dev server on UDP 7445 as the systemd user service `schwitzer-server-dev`. It runs `--auto-update --update-channel dev`, so it installs each build of the `dev` prerelease. Its binary is in `~/.local/state/schwitzer-server-dev/bin` and its data in `~/.local/share/schwitzer-server-dev`. It was installed by hand, not by `deploy/deploy.sh`, whose data dir is shared with the release server. The same rules apply as for the release server. Its server URL is never written into a file of the repo; read it from `ssh stkn.org journalctl --user -u schwitzer-server-dev`, which also holds its logs.

## Running headless

The user runs their own `schwitzer`, with its data in `~/.local/share/schwitzer` (`identity`, `mcp-token`, `settings.toml`) and its MCP server on `127.0.0.1:13339`. An agent never starts a windowed `schwitzer` and never runs it without `--headless` and a `SCHWITZER_HOME` under `/tmp`. That includes `--version`: without these it opens the GUI on the user's identity and rooms. Every headless instance gets its own `SCHWITZER_HOME` with a `settings.toml` that sets its own `mcp_port` (never 13339) and an `omp_sessions_dir` under `/tmp`; otherwise it shares the agent's own omp sessions. A server for a test runs from `target/` with `--data-dir` under `/tmp`.

There is no local server. Tests never use the stkn.org server and never stop, restart or replace it. A test server runs from `target/` on loopback (the examples use 7454).

```sh
cargo build --workspace --features schwitzer-app/remote
mkdir -p /tmp/sw/a /tmp/sw/omp
setsid target/debug/schwitzer-server --listen 127.0.0.1:7454 --data-dir /tmp/sw/server >/tmp/sw/server.out 2>/tmp/sw/server.log &   # server.out: `server url: …`
printf 'name = "alice"\nmcp_port = 13401\nomp_sessions_dir = "/tmp/sw/omp"\n' >/tmp/sw/a/settings.toml
SCHWITZER_HOME=/tmp/sw/a setsid target/debug/schwitzer --headless --control /tmp/sw/a.sock --size 400x780 --backend tiny-skia 2>/tmp/sw/a.log &
iced-impeccable ctl /tmp/sw/a.sock tree                     # or target/debug/schwitzer ctl …; also tap #id / tap ~text, type, wait-for, wait-gone, clip, quit
iced-impeccable ctl /tmp/sw/a.sock screenshot --annotate /tmp/sw/a.png   # look at it with Read
```

Start hosts with `setsid … &`: a plain `&` dies with the harness's shell. `ctl -` reads a batch of commands from stdin; `~/code/iced_impeccable/README.md` lists every command. If `iced-impeccable` is missing, install it with `cargo install --path ~/code/iced_impeccable --bin iced-impeccable --locked`. A control that can only be reached by coordinates gets a widget id (`container(...).id("...")`) instead. Signals to the agent's own processes go by PID, never by name: `pkill schwitzer` would hit the user's app.

A headless instance opens no folder dialog: the folder buttons (`#repo-browse`, `#projects-dir-browse`) answer with `$SCHWITZER_PICK_FOLDER` from the app's environment, or act as cancelled when it is unset.

The MCP server of a headless instance is at `http://127.0.0.1:<mcp_port>/mcp`, with `Authorization: Bearer <$SCHWITZER_HOME/mcp-token>` and `Accept: application/json, text/event-stream`. It answers 401 without the bearer and 403 to any request with an `Origin` header.

## Architecture

| Crate | Role |
|---|---|
| `crates/schwitzer-proto` | Wire types shared by server and client. `rpc.rs`: `Request`, `Response`, `ApiError` (its text is shown verbatim in the UI and over MCP); `events.rs`: `Subscribe`, `Event`; `model.rs`: `Stage`, `Priority`, `Work`, `Decision`, `Project`, `Task`, `RoomState`, …; `ids.rs`: `MemberId`, `RoomId`, `RoomToken`, `ClaimToken`; `addr.rs`: `ServerAddr` (`weida://sha256:<fp>@host:port`) and `RoomKey` (`schwitzer://sha256:<fp>@host:port/<token>`); `frame.rs`: `u32 LE length ‖ postcard` frames; `lib.rs`: `PROTOCOL_VERSION`, paths `/rpc` and `/events`, size caps (`caps`), `RPC_TIMEOUT` |
| `crates/schwitzer-server` | Binary `schwitzer-server` and library (`start(Options)`, used by the client's flow test). `rpc.rs` serves `/rpc` (validation, one store transaction per mutation, events published after commit); `events.rs` serves `/events` (snapshot per room, `Ready`, the announced release, then live frames); `hub.rs`: per-member subscriptions with pre-encoded frames and the in-memory `Sessions`; `debug.rs`: the running debug sessions (`DebugRuns`, one per room, relayed and never stored); `limit.rs`: the request budget and the debug log budget in bytes (`ByteBudget`); `release.rs`: every 2 min reads the GitHub release list (`--releases-url`, `Options::releases`; tests pass `None`) and announces `Event::Release`, the newest stable release, whatever the server's own version and channel, and with `--update-channel dev` also asks `releases/tags/dev` every third check (`newest_dev`); `update.rs`: `--auto-update` (`AutoUpdate`, `--update-channel release|dev`, default release) installs the newest build of its channel (the newest stable release, or the highest version on the rolling `dev` prerelease) as `schwitzer-server-<v>-linux-<arch>.tar.gz` (sha256 from the release list) over the program file (download, check and swap through `schwitzer-release`), and `Server::stopped` returns `Stop::Updated` so `main` execs it; `store.rs`: SQLite (`<data-dir>/schwitzer.db`, WAL, `user_version` = `SCHEMA_VERSION`); `auth.rs`: the member id is the client's QUIC key fingerprint, no accounts. The identity lives in `<data-dir>/identity` |
| `crates/schwitzer-client` | `Client`, with its own tokio runtime (`schwitzer-net`) and weida runtime: rooms, requests, a `broadcast` of `ClientEvent`s. `conn.rs`: weida `Requester`s for `/rpc` and `/events`, server key pinned by the fingerprint in the address; `sync.rs`: per-server loop (dial, `Hello`, rejoin rooms, apply events, reconnect with `backoff.rs`, 1 s to 30 s) |
| `crates/schwitzer-release` | What the app's self-update and the server's `--auto-update` share, without iced, SQLite, weida or tokio: `RELEASES_URL`, `PLATFORM` (asset suffix of this build), `GitHubRelease` (a release as the API lists it), `Asset` and `Download` (the `sha256:` digest GitHub lists), `parse_tag`/`is_stable`, `VERSION`/`version()`/`user_agent(program)` (this build's version: `SCHWITZER_VERSION` at build time, else the workspace version), `is_update` (a newer version always; an older one only across stable/dev), `Channel` (`release`/`dev`), `DEV_TAG`, `newest_asset` (the highest `<prefix><version><suffix>` among a release's assets), `api_get` (GitHub API request with its two headers, bounded, 30 s), `api_get_dev` (the `tags/dev` release, with the "no dev release is on GitHub" refusal) and `fetch` (download bounded, 600 s, then size and sha256 checked; user agent and limit are parameters), `extract_tar_gz` (Unix) and `extract_zip` (Windows), `sibling`, `replace` (Unix: `.new`, chmod 0755, rename; Windows: the old file moves to `.old`) and `clean_leftovers` (`.new`, `.old`) |
| `crates/schwitzer-app` | Binary `schwitzer`, iced 0.14. `app/mod.rs`: `Message`, `App`, `update`, `subscription`; `app/logic.rs`: pure logic without iced (task groups, `in_repos`, pick prompt); views in `app/{list,task_view,licenses_view,chrome,live,main_view,widgets,debug_view,pages,sticky}.rs` and `app/settings_view/` (the Settings overview and one page per topic), icons in `app/icons.rs` (`assets/icons/`, Lucide); `about.rs`: the license texts of the About page (own `LICENSE-MIT`/`LICENSE-APACHE`, Figtree, Lucide, `assets/licenses/third-party.json` from `examples/licenses.rs`, whose test fails when `Cargo.lock` moved on); `theme.rs`: Daylight/Night tokens, Figtree fonts, widget styles; `bridge.rs`: client events into iced (debug log bytes stay out); `settings.rs`: `SCHWITZER_HOME`, `settings.toml` (`update_channel`: `release` or `dev`, absent follows the build); `sessions.rs`: omp session scanner; `debug.rs`: the debug session worker per joined session, which streams the files of `<data dir>/debug/<session>/out` to the other participants and writes theirs to `in/<member dir>/` (`Outbox`, `Inbox`, `Traffic`); `wsl.rs`: on Windows, the running WSL distros (`wsl.exe --list --running`; a stopped distro is never touched by periodic work), their projects folders and omp sessions, and the path mapping between Windows and the distro (`\\wsl.localhost\<distro>\…` ↔ `/…`, `C:\…` ↔ `/mnt/c/…`); `mcp/`: MCP server for the omp plugin (rmcp on axum, `serve.rs`, bearer token `token.rs`, tools `tools.rs`); `omp_plugin.rs`: writes the omp plugin `omp-plugin/index.ts` (described in `omp-plugin/README.md`) (`/schwitzer plan|build|propose|idea|debug`: claim a task for planning and send the plan with `submit_plan`, claim an approved task to build, draft a proposal or write up an idea, note an idea with `add_idea`, start or join a debug session whose instructions reach the agent only through the human's editor) and the agent tools `schwitzer_*` into a shared repo's `.omp/plugins/` and removes it when no project is linked to the repo any more; every 5 s the app compares each installed plugin with what it would write (`omp_plugin::state`) and rewrites an out-of-date one by hand or, with `auto_update_plugins`, automatically. `update.rs`: self-update on the member's channel (`Lookup`). Release channel: the release the servers announced (`ClientEvent::Release`; the highest one over the online servers of the kept rooms) or, when none did, GitHub's latest release, fetched from GitHub (`releases/tags/v<version>` or `releases/latest`). Dev channel: the highest `schwitzer-<version>-<platform>.<ext>` asset of the rolling `dev` prerelease (`releases/tags/dev`, `parse_dev`), announcements ignored. Switching the channel installs the newest build of the new one, even an older one (dev to release is a downgrade); then download, sha256 against the release's asset digest, swap the program file (the `.AppImage` itself when it runs from one, `update::Program`), restart; the download, check, `.tar.gz` member and Unix swap are `schwitzer-release`; retried every 5 min; `SCHWITZER_UPDATE_URL` replaces the GitHub API URL `…/releases` for a local test feed; `app/plugins.rs`: the omp plugin lifecycle in the shared repos (`impl App`: install, update, remove, the periodic `check_plugins`, `sync_plugins`); `app/updates.rs`: the self-update state machine (`impl App`: release target, check, install; the pure decision is `logic::next_check`, the channel `App::channel`; Settings → Updates has the Release/Dev switch); `app/debug_session.rs`: the debug session lifecycle in the app (`impl App`: start, join, leave, the worker subscriptions of the joined sessions and their counts, the notices of a session change and of an instruction); `app/rooms.rs`: `Rooms`, the window's copy of `Client::rooms()` with lookups by room id; `app/resume.rs`: the `resume.json` snapshot (page, room, open task or draft, unsaved text, focused input) the restarted process opens; the project picked per room lives in `settings.toml` (`chosen_projects`). Feature `self-update` turns the updater on (release builds from CI; `auto_install_updates`, `auto_restart_updates`); feature `remote` adds headless control through `iced_impeccable`. `deploy/package.sh` packs the release assets: `.tar.gz`/`.zip`, the Linux AppImage and the Windows installer (`deploy/schwitzer.iss`) |

Data flow: the app's `Client` dials a server's `/rpc` and `/events` over weida (QUIC, mutual TLS). A mutation runs in one SQLite transaction on the server, which then publishes events to every member of the room in commit order. The client folds them into its room state and emits `ClientEvent`s, which `bridge::client_events` turns into `Message`s; a lagging receiver resyncs from `Client::rooms()`. After a reconnect the server sends a full `Event::Room` per room, then `Ready`. omp agents in shared repos talk to the member's own app through the omp plugin, whose `schwitzer_*` tools forward to the app's MCP tools (`list_rooms`, `list_tasks`, `get_task`, `propose`, `add_idea`, `claim`, `submit_plan`, `finish`, `unclaim`, `add_note`, `debug_status`, `debug_instruct`; `debug_start`, `debug_join`, `debug_leave` and `debug_inbox` serve only the plugin's `/schwitzer debug`), which use the same `Client`. Every 5 s `sessions.rs` scans `omp_sessions_dir` (default `~/.omp/agent/sessions`) and, on Windows, `~/.omp/agent/sessions` in every running WSL distro, whose Linux working directories it maps to Windows paths. It links a session to a task by the `schwitzer-claim: <token>` line from `claim` or the pick prompt. It publishes only sessions whose working directory lies in a repository linked to one of that room's projects (`settings.repos`, keyed `<room id hex>/<project id>`). The server keeps sessions in memory only, and debug sessions too: it relays their log bytes and instructions without storing them.

Invariants:
- postcard is positional: enum variants in `rpc.rs`, `events.rs` and `model.rs` are append-only. An incompatible change bumps `PROTOCOL_VERSION`.
- The store maps enums to explicit integers (`stage_to_int`, …), never to their postcard encoding. A schema change bumps `SCHEMA_VERSION`, and `open` refuses an unknown version.
- Claim selection and its update stay in one transaction.
- Agents never decide. Accept, reject, approve, replan, reopen, mark planned and mark done exist only as buttons in the UI, never as MCP or plugin tools.
- The MCP server stays loopback-only, behind a bearer token, and refuses `Origin`; only its port is configurable (`mcp_port`). The token never reaches a log (`McpKey` has no `Debug`).
- Sessions outside the configured repositories never leave the machine.
- Everything from the network and from omp files is untrusted: bounded sizes (`caps`, `MAX_RPC_BYTES`, `MAX_EVENT_FRAME`), and malformed input is an error, never a panic.

Keep pure logic (`app/logic.rs`, `store.rs`, `proto`) free of iced types so it stays unit-testable. That is where most tests live; `sessions.rs` tests read `crates/schwitzer-app/tests/fixtures/*.jsonl`.

## Conventions

- Code, comments, commit messages and repo docs in English. The user writes to you in German.
- Commit messages: imperative subject ≤ 72 chars without any prefix (no `feat:`, `fix:`, `server:`, `app:`). For user-visible changes add a `Changelog: added|changed|deprecated|removed|fixed|security|performance` trailer; the subject becomes the changelog entry. See CONTRIBUTING.md.
- Errors: `thiserror` in the libraries (`schwitzer-proto`, `schwitzer-client`), `anyhow` in `schwitzer-server`; the app shows errors as text (`ApiError`'s `Display`). Log with `tracing`, to stderr: stdout belongs to the server's `server url:` line and to `ctl` replies. Poisoned mutexes are taken over with `unwrap_or_else(PoisonError::into_inner)`.
- `unsafe_code` is forbidden workspace-wide.
- UI work reads DESIGN.md (normative, with Daylight and Night tokens) and PRODUCT.md first and uses the impeccable skill (`.omp/skills/impeccable`). Only the style functions in `theme.rs` read the tokens; views never hard-code colours or sizes.
- `weida` and `iced_impeccable` are pinned to exact git revisions in the root `Cargo.toml`. weida is alpha and its wire protocol may break within 0.x; moving the rev is its own change.
- `SCHWITZER_PLAN.md` (the first cut's plan) and other local planning files stay untracked; the code is authoritative where they differ.

## Environment gotchas

- The dev machine is WSL2/WSLg. Headless instances use `--backend tiny-skia`, which needs no GPU.
- The server listens on UDP (QUIC). `--public-host` is the name in its certificate and in the printed URL, so it must be the name clients dial. `--data-dir` defaults to `./schwitzer-data` relative to the working directory: always pass it.
- `RUST_LOG` filters both binaries (server default `info`, app `info,iced_winit=warn`). Neither writes a log file.
- A bad `settings.toml` loads as defaults with a notice and is not overwritten until the next save.
