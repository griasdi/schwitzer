# AGENTS.md

Schwitzer is a task room for coding agents and their humans: a desktop app in Rust and iced (Linux, Windows, macOS) with a local MCP server for the agents, and a standalone room server over QUIC (weida). PRODUCT.md says what it is for, DESIGN.md how it looks.

## Commands

```sh
cargo build --workspace --features schwitzer-app/remote   # server + app with headless remote control
target/debug/schwitzer-server --listen 127.0.0.1:7454 --data-dir /tmp/sw/server   # prints `server url: weida://sha256:…@host:port` on stdout; also --public-host
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
deploy/deploy.sh local   # build HEAD, run it as the systemd user service schwitzer-server-local on 127.0.0.1:7444
deploy/deploy.sh stkn    # the same on stkn.org (ssh felix@stkn.org): service schwitzer-server, UDP 7444, only when the user asks
```

Never `cargo run -p schwitzer-app` or start `schwitzer` without `--headless`: see "Running headless". There is no CI and no xtask, so the gate below is the only check before a push.

## Gate

Every change that touches code passes the gate before it is committed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features schwitzer-app/remote -- -D warnings
cargo test --workspace
```

After the push, every change that touches code ends with `deploy/deploy.sh local` (see "Finishing a change"): the local server always runs the latest `main`.

A change to doc comments also runs `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`. A UI change is checked the way a user meets it: clicked through headless and screenshotted, not only compiled. `crates/schwitzer-client/tests/flow.rs` starts a real server on loopback UDP and restarts it on the same data dir; it needs no network beyond `127.0.0.1`.

## Finishing a change

A change is finished when its gate passed, it is committed and pushed to `origin main`, and `deploy/deploy.sh local` ran successfully afterwards. A change without code, such as documentation only, skips the gate but is still committed and pushed. `deploy/deploy.sh local` builds the committed `HEAD` (uncommitted work in the checkout is ignored), installs it and restarts the systemd user service `schwitzer-server-local` only when the binary or unit changed, so running it after every code push is cheap. Run it from this checkout on `main` after the push. If it fails, the change is not finished: fix the cause; if `127.0.0.1:7444` is taken by a process the agent did not start, leave it alone and tell the user.

- Stage paths explicitly, never `git add -A` or `git add .`: the checkout may hold the user's uncommitted work, which is left alone. `SCHWITZER_PLAN.md` and other local planning files are never committed; `.omp/mcp.json` holds a token and stays untracked.
- Commit messages follow CONTRIBUTING.md: no prefix, a `Changelog:` trailer for user-visible changes.
- Before pushing, `git pull --rebase origin main`; if that brought new commits, run the gate again. Push only to `origin`.
- A change without code skips `deploy.sh` as well.
- `deploy/deploy.sh stkn` deploys to the shared server on stkn.org (service `schwitzer-server`, `weida://…@stkn.org:7444`), which other people use: run it only when the user asks for it, never as part of finishing a change.

## Running headless

The user runs their own `schwitzer`, with its data in `~/.local/share/schwitzer` (`identity`, `mcp-token`, `settings.toml`) and its MCP server on `127.0.0.1:13339`. An agent never starts a windowed `schwitzer` and never runs it without `--headless` and a `SCHWITZER_HOME` under `/tmp`. That includes `--version`: without these it opens the GUI on the user's identity and rooms. Every headless instance gets its own `SCHWITZER_HOME` with a `settings.toml` that sets its own `mcp_port` (never 13339) and an `omp_sessions_dir` under `/tmp`; otherwise it shares the agent's own omp sessions. A server for a test runs from `target/` with `--data-dir` under `/tmp`.

The user's local server is the systemd user service `schwitzer-server-local` on `127.0.0.1:7444` (data in `~/.local/share/schwitzer-server`, binary in `~/.local/state/schwitzer-server-local/bin`, logs via `journalctl --user -u schwitzer-server-local`). Tests never use it and never stop, restart or replace it other than through `deploy/deploy.sh local`; a test server listens on another port (the examples use 7454).

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

The MCP server of a headless instance is at `http://127.0.0.1:<mcp_port>/mcp`, with `Authorization: Bearer <$SCHWITZER_HOME/mcp-token>` and `Accept: application/json, text/event-stream`. It answers 401 without the bearer and 403 to any request with an `Origin` header.

## Architecture

| Crate | Role |
|---|---|
| `crates/schwitzer-proto` | Wire types shared by server and client. `rpc.rs`: `Request`, `Response`, `ApiError` (its text is shown verbatim in the UI and over MCP); `events.rs`: `Subscribe`, `Event`; `model.rs`: `Stage`, `Priority`, `Work`, `Decision`, `Project`, `Task`, `RoomState`, …; `ids.rs`: `MemberId`, `RoomId`, `RoomToken`, `ClaimToken`; `addr.rs`: `ServerAddr` (`weida://sha256:<fp>@host:port`) and `RoomKey` (`schwitzer://sha256:<fp>@host:port/<token>`); `frame.rs`: `u32 LE length ‖ postcard` frames; `lib.rs`: `PROTOCOL_VERSION`, paths `/rpc` and `/events`, size caps (`caps`), `RPC_TIMEOUT` |
| `crates/schwitzer-server` | Binary `schwitzer-server` and library (`start(Options)`, used by the client's flow test). `rpc.rs` serves `/rpc` (validation, one store transaction per mutation, events published after commit); `events.rs` serves `/events` (snapshot per room, `Ready`, then live frames); `hub.rs`: per-member subscriptions with pre-encoded frames and the in-memory `Sessions`; `store.rs`: SQLite (`<data-dir>/schwitzer.db`, WAL, `user_version` = `SCHEMA_VERSION`); `auth.rs`: the member id is the client's QUIC key fingerprint, no accounts. The identity lives in `<data-dir>/identity` |
| `crates/schwitzer-client` | `Client`, with its own tokio runtime (`schwitzer-net`) and weida runtime: rooms, requests, a `broadcast` of `ClientEvent`s. `conn.rs`: weida `Requester`s for `/rpc` and `/events`, server key pinned by the fingerprint in the address; `sync.rs`: per-server loop (dial, `Hello`, rejoin rooms, apply events, reconnect with `backoff.rs`, 1 s to 30 s) |
| `crates/schwitzer-app` | Binary `schwitzer`, iced 0.14. `app/mod.rs`: `Message`, `App`, `update`, `subscription`; `app/logic.rs`: pure logic without iced (task groups, `in_repos`, pick prompt); views in `app/{list,task_view,settings_view,chrome,live,widgets}.rs`, icons in `app/icons.rs` (`assets/icons/`, Lucide); `theme.rs`: Daylight/Night tokens, Figtree fonts, widget styles; `bridge.rs`: client events into iced; `settings.rs`: `SCHWITZER_HOME`, `settings.toml`; `sessions.rs`: omp session scanner; `mcp/`: MCP server for the omp plugin (rmcp on axum, `serve.rs`, bearer token `token.rs`, tools `tools.rs`); `omp_plugin.rs`: writes the omp plugin `omp-plugin/index.ts` (described in `omp-plugin/README.md`) (`/schwitzer plan|build|propose|idea`: claim a task for planning and send the plan with `submit_plan`, claim an approved task to build, draft a proposal or write up an idea, note an idea with `add_idea`) and the agent tools `schwitzer_*` into a shared repo's `.omp/plugins/` and removes it when no project is linked to the repo any more; every 5 s the app compares each installed plugin with what it would write (`omp_plugin::state`) and rewrites an out-of-date one by hand or, with `auto_update_plugins`, automatically. Feature `remote` adds headless control through `iced_impeccable` |

Data flow: the app's `Client` dials a server's `/rpc` and `/events` over weida (QUIC, mutual TLS). A mutation runs in one SQLite transaction on the server, which then publishes events to every member of the room in commit order. The client folds them into its room state and emits `ClientEvent`s, which `bridge::client_events` turns into `Message`s; a lagging receiver resyncs from `Client::rooms()`. After a reconnect the server sends a full `Event::Room` per room, then `Ready`. omp agents in shared repos talk to the member's own app through the omp plugin, whose `schwitzer_*` tools forward to the app's MCP tools (`list_rooms`, `list_tasks`, `get_task`, `propose`, `add_idea`, `claim`, `submit_plan`, `finish`, `release`, `add_note`), which use the same `Client`. Every 5 s `sessions.rs` scans `omp_sessions_dir` (default `~/.omp/agent/sessions`). It links a session to a task by the `schwitzer-claim: <token>` line from `claim` or the pick prompt. It publishes only sessions whose working directory lies in a repository linked to one of that room's projects (`settings.repos`, keyed `<room id hex>/<project id>`). The server keeps sessions in memory only.

Invariants:
- postcard is positional: enum variants in `rpc.rs`, `events.rs` and `model.rs` are append-only. An incompatible change bumps `PROTOCOL_VERSION`.
- The store maps enums to explicit integers (`stage_to_int`, …), never to their postcard encoding. A schema change bumps `SCHEMA_VERSION`, and `open` refuses an unknown version.
- Claim selection and its update stay in one transaction.
- Agents never decide. Accept, reject, approve, replan and reopen exist only as buttons in the UI, never as MCP or plugin tools.
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
