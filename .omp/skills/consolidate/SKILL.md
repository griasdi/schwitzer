---
name: consolidate
description: Review the Schwitzer repository (or the crate or path given as argument) as if it were built from scratch today, pull duplicated code and state into one place, finish half-done migrations, align rival conventions, remove dead weight and stale docs, and land each consolidation as its own gated commit on main, pushed to origin main, together with a run log in docs/consolidations/; changes to observable behavior, the wire protocol, stored formats or external interfaces (CLI, settings, MCP tools, the omp plugin) only after the user approves them.
---

# consolidate

Goal: bring Schwitzer back to the shape it would have if it were written today. For every area the question is "how would we structure this today, knowing what Schwitzer has become?", not "what is the smallest diff?". Duplicated code moves into shared modules, rival conventions become one, dead weight goes.

Consolidation preserves behavior. It is not a feature run and not a bug-fix run: what the app and the server do before and after a consolidation commit is the same, unless the user approved a specific behavior or compatibility change (step 5). A bug found during the review is a `behavior` finding.

This skill is in English; messages to the user are in the language the user writes in.

`AGENTS.md` applies in full: the gate, "Finishing a change", "Running headless" and the invariants. The consequences that matter most here:

- A security problem found during the review is reported to the user (step 8), never fixed on the side. Schwitzer is public: the repository, its releases and its run logs.
- Agents never decide. No consolidation turns a UI-only decision (accept, reject, approve, replan, reopen, mark planned, mark done) into an MCP or plugin tool, and the run never claims, finishes or decides a task of the room.
- stkn.org is never deployed, restarted or replaced, and `deploy/deploy.sh` is never run.
- weida, iced_impeccable and omp are not changed from here, and their pins in the root `Cargo.toml` do not move in a consolidation (step 5).
- `SCHWITZER_PLAN.md`, `.omp/mcp.json` and other local untracked files are never edited, deleted, moved or added to git.

## 1. Arguments and target repository

- Run `git rev-parse --show-toplevel` in the current directory. The main checkout is the first `worktree` entry of `git worktree list --porcelain`. If it contains `crates/schwitzer-server/`, it is the target repository; otherwise use `~/code/schwitzer`. Below, `<repo>` is that main checkout.
- Optional argument: a crate name (`schwitzer-app`) or a repository-relative path (`crates/schwitzer-app/src/mcp`, `deploy`).
- Without an argument the scope is the whole repository: every crate under `crates/` (with `omp-plugin/`, `examples/`, `assets/` and `tests/`), `deploy/`, `.github/`, `site/`, `docs/`, `design/logo/README.md` and the tracked root files (`*.md`, `Cargo.toml`, `deny.toml`, `.gitignore`). `.impeccable/` and `.omp/skills/` are not reviewed: they belong to the design skill and to the skills themselves.
- With an argument, duplicates across crates are still searched, but a finding is only consolidated if at least one side lies in the scope.
- Unknown crate or nonexistent path: tell the user that the repository has no crate or path `<arg>`, and stop.

## 2. Workspace

- `git -C <repo> fetch -q origin`.
- Record the user's uncommitted work: the paths `git -C <repo> status --porcelain --untracked-files=no` prints. Step 5 asks before touching them.
- If that printed anything, or `<repo>` is not on `main`, work in a worktree: `git -C <repo> worktree add <repo>/../schwitzer-consolidate -b consolidate-<YYYY-MM-DD> origin/main` (append `-2`, `-3` and so on to path and branch if they exist). The worktree has its own `target/`, so the first build is cold; do not point `CARGO_TARGET_DIR` at the main checkout's `target/`.
- Otherwise work in `<repo>` itself, on a new branch `consolidate-<YYYY-MM-DD>` from `origin/main`.
- The branch stays local. It lands on `main` by rebase in step 7 and is never pushed itself. Push only to `origin`.
- Scratch space for the whole run is `/tmp/schwitzer-consolidate/` (headless homes, server data, screenshots). Nothing in this run reads or writes `~/.local/share/schwitzer`, port 13339 or stkn.org.
- Before the first change, on the starting commit: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets --features schwitzer-app/remote -- -D warnings` and `cargo test --workspace`. If `cargo.exe` is on `PATH`, also the Windows check of step 6 on the starting commit. What fails there goes into the run log as pre-existing; it is neither blamed on nor fixed inside a consolidation commit.

## 3. Orientation

Read before reviewing:

- `AGENTS.md`, `CONTRIBUTING.md`, `docs/ARCHITECTURE.md` (completely, in particular section 3 "Workspace and crates", section 12 "Invariants" and section 13 "Where to change what"), `PRODUCT.md`, `README.md` and the root `Cargo.toml`. `DESIGN.md` is normative for every UI finding; read the parts a UI finding touches.
- The planned direction is in the room, not in files. Call `schwitzer_list_rooms`, then `schwitzer_list_tasks` for every room that has a project named `schwitzer`, limited to that project, and `schwitzer_get_task` for every task in stage proposed, accepted, planned, ready or in progress. A consolidation must not fight a task that is still open. A task only protects code while it is open: one the code already implements, or whose direction the code has since left, protects nothing and is mentioned in the chat report (step 8); the run never changes the task. Ideas protect nothing. For every task in progress, note the files its plan names (step 5). If the Schwitzer tools are unavailable (the user's app is not running), tell the user, continue without the room and say so in the run log.
- `SCHWITZER_PLAN.md` is the plan of the first cut, not the direction: where it and the code differ, the code is authoritative. It is not used for triage.
- `git log --oneline <base>..HEAD` and `git diff --stat <base>..HEAD`, where `<base>` is the review base of the newest `docs/consolidations/*.md` (without one, `git log --oneline -40` and `git diff --stat HEAD~40..HEAD`): what moved since the last run. Step 4 spends scouts in proportion to this churn: the slices with the most changed lines get the most attention, and every scout reviews the code added since `<base>` first.
- Every earlier run log, `docs/consolidations/*.md`: what was consolidated, dropped or declined before. A finding declined in an earlier run is not proposed again unless something it depended on has changed since; then name that change in the `ask`.

## 4. Review (parallel, read-only)

Spawn `scout` subagents in one batch. Slices for the whole repository:

1. App core: `app/mod.rs` (`Message`, `App`, `update`, `subscription`), `app/resume.rs`, `app/gpu.rs`, `app/pages.rs`, `app/sticky.rs`, `bridge.rs`, `main.rs`, `system_notify.rs`, `brand.rs` of `schwitzer-app`
2. App views and look: `app/list.rs`, `app/task_view.rs`, `app/licenses_view.rs`, `app/chrome.rs`, `app/live.rs`, `app/widgets.rs`, `app/icons.rs`, `app/settings_view/`, `theme.rs`, `about.rs`, `examples/`, `assets/`, checked against `DESIGN.md`
3. App logic and local services: `app/logic.rs`, `settings.rs`, `sessions.rs` (with `tests/fixtures/`), `wsl.rs`, `update.rs`, `build.rs`, `Cargo.toml` of `schwitzer-app`
4. Agent integration: `mcp/` (`serve.rs`, `token.rs`, `tools.rs`), `omp_plugin.rs`, `omp-plugin/index.ts`, `omp-plugin/README.md`
5. `schwitzer-client`, including `tests/flow.rs`
6. `schwitzer-server`
7. `schwitzer-proto`
8. Tooling, packaging and site: `deploy/`, `.github/workflows/`, root `Cargo.toml`, `deny.toml`, `crates/schwitzer-app/about.toml`, `.gitignore`, `site/`, `design/logo/README.md`
9. Cross-cutting: the same helper, type, constant, environment variable name, error type or pattern in more than one crate (release lists, downloads and checksum checks, backoff and intervals, size limits, path handling); a crate pinning its own version where `[workspace.dependencies]` has the dependency; duplicate dependency versions (`cargo tree -d`); dependencies a crate's `Cargo.toml` names but its code never uses (grep the crate for the dependency's `use` path, mind renamed, `cfg`-target and `features`-only dependencies); crate boundaries that break the "Must not know about" column of `docs/ARCHITECTURE.md` section 3; the same platform split (`cfg(windows)`, `target_os = "macos"`) written differently in several places.
10. Documentation and configuration: `docs/ARCHITECTURE.md` (sections 12 and 13 checked against the code), `docs/architecture.svg`, `docs/harness.svg`, `AGENTS.md` (its commands and the Architecture table against `docs/ARCHITECTURE.md` section 3 and the code), `CONTRIBUTING.md`, `README.md`, `PRODUCT.md`, `DESIGN.md` against the tokens in `theme.rs`, `omp-plugin/README.md` against `index.ts` and `mcp/tools.rs`, `site/index.html` against `README.md`. Findings: documents describing an older architecture or a removed path, tables that disagree with each other or with the code, settings whose defaults, validation, example and documentation disagree, and settings nothing reads.

Files and crates added since this list was written join the slice they fit best. With an argument: one scout per top-level module of the scope (at most 8), plus the cross-cutting and the documentation scout, both limited to findings that touch the scope.

Every scout gets the same checklist:

The evidence and intent fields below follow studies of agents refactoring real code (arXiv 2605.07001, SmellBench: 63% of detector hits were false positives; arXiv 2608.23611, REFINE; arXiv 2507.03536, ACE): a finding the scout cannot reproduce, or that the code does on purpose, is not reported.

- **Duplicates:** functions, types, constants, widget compositions, style functions, SQL, error mapping, retry and backoff, size checks, path handling (Windows and WSL path mapping, `~` expansion), release-list parsing, downloads and sha256 checks that exist twice or more, including near-duplicates that differ only in names or a parameter. Not a finding: code that looks alike but serves concepts that will change independently.
- **Duplicate state:** the same fact held in two places (a field that another field or a lookup already determines, room state the app keeps beside `Client::rooms()`, a value in both `settings.toml` and `resume.json`, a cache with no invalidation reason), repeated conversions between near-identical types, adapters between two abstractions that should be one. Name the single source of truth.
- **Half-finished migrations:** an old and a new way both still in use, where the code shows a move from one to the other that stopped part-way. The newer, deliberate direction wins; every remaining caller moves and the old way is deleted.
- **Rival conventions:** two ways of doing the same thing (two error styles, two settings readers, two widget patterns, two logging styles). Name the one that should win and why.
- **Bolted-on features:** a feature handled as a special case beside the core model (its own arm in `update` of `app/mod.rs`, its own branch in a view or in the server's `dispatch`, its own flag threaded through unrelated code) where it should be one case of the model, state and lifecycle that already exist.
- **Misplaced code:** logic in the wrong crate or module, for example iced-free logic in a view that belongs in `app/logic.rs`, protocol rules in the app that belong in `schwitzer-proto` or the server, colours or sizes read outside the style functions of `theme.rs`, network access from the app outside `schwitzer-client`.
- **Stale names:** types, functions, modules, fields, messages whose name describes earlier behavior and now misleads. Only where the meaning has drifted; a name that is merely not pretty is not a finding.
- **Speculative abstractions:** traits, generics, builders, extension points with one implementation and no credible second one. Keep abstractions that enforce a real boundary or invariant (the iced-free `app/logic.rs`, the `schwitzer-client` boundary that keeps SQLite and weida out of the app); do not collapse useful boundaries to save files.
- **Error handling:** errors swallowed without reason (`let _ =`, `.ok()`, empty `Err(_) => {}`), vague messages without context, `anyhow` in `schwitzer-proto` or `schwitzer-client` or ad-hoc error types in `schwitzer-server`, `println!`/`eprintln!` where `tracing` belongs (stdout is reserved for the server's `server url:` line and `ctl` replies), mutex poisoning handled other than with `unwrap_or_else(PoisonError::into_inner)`. Surfacing a swallowed error changes behavior: such findings take `compat: behavior`. A panic reachable from network or omp input is a security observation (step 8), not a finding.
- **Dead weight:** unused code, compatibility shims, aliases, re-exports kept for old callers, stale features and `cfg` branches nothing enables, `#[allow(dead_code)]` hiding dead items, icons and assets nothing includes, dead files and scripts, temporary debug code.
- **Comments and TODOs:** comments that narrate history ("previously", "now", "new") or explain a workaround whose cause is gone. Every `TODO`/`FIXME`/`XXX` in the slice is classified: still valid and actionable (stays, rewritten if vague), already done another way, obsolete, or too vague to act on (the last three are deleted). Comments on invariants, constraints, platform quirks and trade-offs stay.
- **Tests:** tests that pin obsolete internals, intermediate states of earlier development or private structure instead of behavior; tests that duplicate each other; tests that reach beyond loopback or outside a temp directory.
- **Greenfield deviations:** module or crate boundaries, types that should own an invariant, oversized files or functions that mix concerns.

Each scout returns its findings as one JSON object with an array `findings`, each item with

- `title` (string)
- `kind`: `duplicate` | `state` | `migration` | `convention` | `bolted` | `misplaced` | `naming` | `abstraction` | `error` | `dead` | `comment` | `test` | `docs` | `structure`
- `locations`: array of `path:line` strings
- `proposal` (string): target module or crate, new symbol names and signatures
- `compat`: `internal` | `behavior` | `wire` | `stored` | `interface`
- `size`: `small` (< 150 changed lines) | `medium` (< 800) | `large`
- `evidence` (string): how the finding was established, reproducibly: the command and the output line or hit count (`grep` hits outside the definition, an `xd://lsp` reference count, a `cargo tree` row, a clippy lint name), or for a duplicate the matching `path:line` ranges.
- `intent` (string): one clause on why the code might be deliberate (a trait or framework requires it, a kept duplicate or dropped finding of an earlier run log, a documented trade-off, code only one platform compiles) and why that does not hold here.

The scout justifies the `compat` class of every finding; in doubt it takes the higher class (`wire` above `stored` above `interface` above `behavior` above `internal`):

- `wire`: changes anything the app and the server exchange: the types in `crates/schwitzer-proto/src/` (`rpc.rs`, `events.rs`, `model.rs`, `ids.rs`, `addr.rs`, `frame.rs`), `PROTOCOL_VERSION` (`crates/schwitzer-proto/src/lib.rs`), the `caps` and frame size limits, the paths `/rpc` and `/events`, the order of the event stream (snapshot, `Ready`, release, live frames), the `Hello` handshake in `crates/schwitzer-client/src/sync.rs` and the server's `rpc.rs`, or the use of weida in `conn.rs` and the server.
- `stored`: changes what is written and read back later: the server store (`crates/schwitzer-server/src/store.rs`: `SCHEMA`, `MIGRATE_n`, `SCHEMA_VERSION`, the enum-to-integer maps), the server's `<data-dir>/identity`, the files in `SCHWITZER_HOME` (`identity`, `mcp-token`, `settings.toml`, `resume.json` from `app/resume.rs`), and the files the app writes into linked repositories (`.omp/plugins/`: the plugin, `schwitzer.json`, `omp-plugins.lock.json`; `omp_plugin.rs`).
- `interface`: changes something people, scripts or programs outside the workspace rely on: CLI flags of `schwitzer` and `schwitzer-server` (`main.rs` of each), `settings.toml` keys, defaults and validation (`settings.rs`), environment variables (`SCHWITZER_HOME`, `SCHWITZER_UPDATE_URL`, `SCHWITZER_PICK_FOLDER`, `RUST_LOG`), the MCP tools, their names, arguments and descriptions (`mcp/tools.rs`), the plugin's `schwitzer_*` tools, `/schwitzer` subcommands and prompt texts (`omp-plugin/index.ts`), the `schwitzer-claim: <token>` marker (`sessions.rs`), the `weida://` and `schwitzer://` address formats (`addr.rs`), the `server url:` line on stdout (`deploy/deploy.sh` waits for it), release asset names (`deploy/package.sh`, both `update.rs`), the systemd unit, the arguments of the `deploy/*.sh` scripts, and widget ids that `AGENTS.md` or headless scripts name (`#repo-browse`, `#projects-dir-browse`).
- `behavior`: none of the above, but what a user, an agent or a log reader observes changes: a pixel in a screen, UI text, an `ApiError` text (shown verbatim in the UI and over MCP), a notification or log message, a default, a timing (reconnect backoff, scan and check intervals), a result in an edge case, a fixed bug.
- `internal`: none of the above.

## 5. Triage

- Merge the findings and drop duplicates across slices. Verify every finding by reading the cited locations yourself: scouts can be wrong. Re-run each finding's `evidence`; a finding whose evidence does not reproduce, or whose `intent` holds, is dropped.
- Before deciding, write a target picture per reviewed area: three to five sentences on how that area would be built today, knowing what Schwitzer has become (ownership, boundaries, the one representation of each core concept). Findings are judged against it: one that moves the area toward it is kept, one that only moves code around is dropped. The target picture goes into the run log (step 8).
- Drop findings that only rename for taste (a `naming` finding needs a name that now misleads), findings that would rewrite a working subsystem only for stylistic uniformity, findings that would break an invariant of `docs/ARCHITECTURE.md` section 12, findings that would fight a task that is still open (step 3), and findings declined in an earlier run log without a changed reason (step 3).
- `internal` findings: carry out all of them (step 6), except those of the next point. A `docs` finding about a stale section of a tracked document rewrites or deletes that section; nothing is archived in a second place.
- Findings with a location in a file that step 2 recorded as uncommitted in `<repo>`, or in a file the plan of a task in progress names (step 3), whatever their `compat`: ask before carrying them out, in the same `ask` call as the next point, naming the file or the task number and that the user's work or that build will have to rebase over the change. Approved ones join step 6; the others go into the run log as ⏸ open, with the file or task.
- `behavior`, `wire`, `stored` and `interface` findings: never carry out without approval. Ask the user with one `ask` call: one question with `multi: true` per group of at most five findings, one option per finding, with title, what changes or breaks, and what else must change with it. For `wire`: the `PROTOCOL_VERSION` bump, and that apps older than the next release cannot use a server once it runs that release, stkn.org among them (it installs each release itself). For `stored`: a new `MIGRATE_n` and `SCHEMA_VERSION` in `store.rs`, or how existing `settings.toml`, `resume.json` and plugin files are read after the change. For `interface`: the places `docs/ARCHITECTURE.md` section 13 lists (an MCP tool: `mcp/tools.rs`, `omp-plugin/index.ts` and `omp-plugin/README.md`; a setting: `settings.rs` and its page in `app/settings_view/`). Approved ones join step 6; the others go into the run log as declined (step 8).
- Findings that are not carried out here go into the run log (step 8) with their next step:
  - A new external dependency or a new `deny.toml` exception: the user's decision.
  - A change in weida, iced_impeccable or omp, or moving the weida or iced_impeccable pin: the user's decision; they are not changed from here.
  - A change in another Schwitzer project of the rooms from step 3 (for example a dependency): draft a proposal for that project, a one-line title and a Markdown body that says what is needed and why, starting with `Requested from project "schwitzer" in room "<room name>"`. Ask the user with `ask`, title and body in the question, whether to send it; only after the user agrees, call `schwitzer_propose` with that project's room id and name. The run log names the proposal and the task number it got.
  - Changes to `.github/workflows/release.yml`, `.github/workflows/site.yml`, `deploy/deploy.sh`, `deploy/release.sh`, `deploy/package.sh`, `deploy/schwitzer-server.service`, `deploy/schwitzer.iss` or `deploy/schwitzer.desktop`: only a tag push, a site deploy or the maintainer exercises them, so they get a recommendation. `.github/workflows/ci.yml` may change (CI checks it after the push), and so may `deploy/changelog.sh` (checked in step 6).
- A new workspace crate is allowed when shared code is used by two or more crates and has no natural home among the existing ones. It takes the workspace `version`, `edition`, `rust-version`, `license` and lints, joins `[workspace.dependencies]`, and gets a row in the crate table and the crate diagram of `docs/ARCHITECTURE.md` section 3, in the Architecture table of `AGENTS.md` and in the Layout table of `CONTRIBUTING.md`; "the four crates" in the repository map of `docs/ARCHITECTURE.md` changes with it.

## 6. Implement (sequential)

- Order: `dead`, `comment` and `docs` first, then by size `small`, `medium`, `large`. Within that, dependencies first: a shared module lands before its callers move to it, a migration completes before code built on the winning side is consolidated.
- One finding, one commit.
- Implementation runs in the main agent or in one `task` subagent at a time, never in parallel: findings share files and the cargo target lock.

Per finding:

- Behavior stays identical. An approved compat finding changes only what was approved, with the `PROTOCOL_VERSION` bump or migration named in the approval.
- No half states: a commit completes its migration. If a finding cannot be finished within its commit, it is dropped (last point), never landed half-way.
- Use `xd://lsp` references before moving or renaming an exported symbol, and `xd://ast_edit` for mechanical codemods.
- Clean cutover: every caller moves. No re-exports, aliases, deprecated wrappers or "moved to" comments remain.
- Deleting a `pub` item, a test, an `assert!` or an error-handling branch needs zero references across the workspace (`xd://lsp` references, plus `grep` over `docs/`, `deploy/`, `.github/`, `site/`, `crates/schwitzer-app/omp-plugin/` and the root `*.md`); a test or assertion goes only together with the code it covers. Without that proof the finding is `compat: behavior`.
- Tests move with their code; duplicated tests merge into one; tests that pin obsolete internals are deleted, not rewritten to pin the new ones. No new tests, unless the shared module now carries an invariant no existing test covers. Never weaken a behavior test to make a refactor pass.
- Update every document that names a moved path or symbol: `docs/ARCHITECTURE.md` (including section 13 and the repository map), the Architecture table and the commands of `AGENTS.md`, `CONTRIBUTING.md`, `README.md`, `omp-plugin/README.md`, `docs/architecture.svg` and `docs/harness.svg` where they name it, and doc comments.
- Check: `cargo fmt --all`, then for each touched crate `cargo clippy -p <crate> --all-targets -- -D warnings` and `cargo test -p <crate>`. Touched `schwitzer-app`: also `cargo clippy -p schwitzer-app --all-targets --features remote -- -D warnings` and `cargo clippy -p schwitzer-app --all-targets --features self-update -- -D warnings`. Touched `schwitzer-proto` or `schwitzer-server`: also `cargo test -p schwitzer-client` (its flow test runs a real server on loopback). Touched a doc comment: `RUSTDOCFLAGS="-D warnings" cargo doc -p <crate> --no-deps`.
- After the checks pass, re-run the finding's `evidence`: the hit is gone and no new hit of the same kind appeared. Then `git diff --stat` may show only the finding's `locations`, the callers it moves and the documents that name them, within its `size` class. Anything else (formatting of untouched code, renames or simplifications on the side) is reverted or becomes its own finding; a diff that outgrows its size class is dropped, not committed (arXiv 2605.22526 and 2511.04824: agent refactorings tangled into unrelated changes lower compilability without improving correctness).
- Windows: if the commit touches `crates/schwitzer-app/Cargo.toml`, `crates/schwitzer-app/build.rs`, `crates/schwitzer-app/src/main.rs`, or a `.rs` file of `schwitzer-app` with `windows` in a `cfg`, `cfg!` or `cfg_attr` (`wsl.rs`, `update.rs` and `sessions.rs` have them), and `cargo.exe` is on `PATH` (WSL with a Windows Rust toolchain), check the committed state on Windows: `win="$(wslpath "$(cmd.exe /c 'echo %LOCALAPPDATA%' 2>/dev/null | tr -d '\r')")/Temp/schwitzer-consolidate"`, `mkdir -p "$win"`, `find "$win" -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +`, `git archive HEAD | tar -x -C "$win"`, then with `$win` as working directory `cargo.exe clippy -p schwitzer-app --all-targets --features self-update -- -D warnings` and `cargo.exe test -p schwitzer-app --bin schwitzer --features self-update`. If that test already failed on the starting commit (step 2), run it with the filter CI uses, `update::`, for the whole run and note this in the run log. On failure fix and `git commit --amend --no-edit`, or drop the finding (last point, with `git reset --hard HEAD~1`). Without `cargo.exe` the Windows build is only checked by CI after the push, without being waited for. Never use the user's own Windows checkout for this.
- macOS: code under `target_os = "macos"` is not compiled here; CI checks app clippy on macOS after the push, without being waited for. Note such commits under remaining risks in the run log.
- UI-visible code (the views in `app/`, `app/settings_view/`, `theme.rs`, `app/widgets.rs`, `app/icons.rs`, `about.rs`, `assets/`): compare headless screenshots, as in `AGENTS.md` "Running headless". Once per run: `mkdir -p /tmp/schwitzer-consolidate/{a,omp,server,shots}`, `printf 'name = "consolidate"\nmcp_port = 13461\nomp_sessions_dir = "/tmp/schwitzer-consolidate/omp"\n' >/tmp/schwitzer-consolidate/a/settings.toml`. Before editing: `cargo build --workspace --features schwitzer-app/remote`, `setsid target/debug/schwitzer-server --listen 127.0.0.1:7464 --data-dir /tmp/schwitzer-consolidate/server >/tmp/schwitzer-consolidate/server.out 2>/tmp/schwitzer-consolidate/server.log &` (once per run; its `server url:` line is in `server.out`), `SCHWITZER_HOME=/tmp/schwitzer-consolidate/a setsid target/debug/schwitzer --headless --control /tmp/schwitzer-consolidate/a.sock --size 400x780 --backend tiny-skia 2>/tmp/schwitzer-consolidate/a.log &`. Open the affected screen with `iced-impeccable ctl /tmp/schwitzer-consolidate/a.sock` (`tree` first, then `tap`, `type`, `wait-for`; `~/code/iced_impeccable/README.md` lists the commands); a screen that needs a room gets one created through the UI with that server URL, and home and data dir keep it for the after run. Then `appearance light`, `screenshot /tmp/schwitzer-consolidate/shots/<finding #>-<screen>-daylight-before.png`, `appearance dark`, `screenshot …-night-before.png`, `quit`. After editing, rebuild and repeat into `-after.png`; if the finding also touched the server, restart the server from the new build on the same data dir first. Look at each pair with `read`: they must look the same. Stop processes by PID, never by name.
- `site/`: open `file://<checkout>/site/index.html` in a `browser` tab at 1280×800 and at 400×780, full-page screenshot before and after editing, and compare them with `read`; they must look the same.
- `omp-plugin/index.ts`: beside the `schwitzer-app` tests, `bun build --no-bundle crates/schwitzer-app/omp-plugin/index.ts --outdir /tmp/schwitzer-consolidate/plugin` before and after editing; if it already fails before (no `bun`, or the file needs omp's resolver), only the Rust tests count.
- `deploy/changelog.sh`: `deploy/changelog.sh v0.1.0..HEAD` before and after editing, with identical output.
- Commit per `CONTRIBUTING.md`: imperative subject of at most 72 characters without prefix, a blank line, and a body that explains why (which duplicates or conventions were unified). Stage paths explicitly, never with `git add -A` or `git add .`. Internal refactors carry no `Changelog` trailer. Approved compat changes carry `Changelog: changed`, `Changelog: removed` when a setting, tool or flag goes away, or `Changelog: fixed` for an approved bug fix, and their subject is written as a release note users understand.
- If a finding turns out wrong or not behavior-preserving while implementing it, go back to the last commit (`git reset --hard HEAD`), record the finding as dropped with the reason, and continue with the next one.

## 7. Land

- Run the full gate of `AGENTS.md`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets --features schwitzer-app/remote -- -D warnings`, `cargo test --workspace`, `cargo deny check`, plus `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`. If any commit of the run triggered the Windows check of step 6 and `cargo.exe` is on `PATH`, that check once more on `HEAD`.
- `git pull --rebase origin main`. If that brought new commits, run the full gate again.
- Gate not green and not fixable within the consolidation commits: ask the user before going on.
- Write the run log (step 8) now, after the rebase, so it names the final commit hashes, and commit it on its own: subject `Log the consolidation run of <YYYY-MM-DD>`, no `Changelog` trailer. It is documentation only and needs no second gate. The first run log also changes the `docs/` row of the repository map in `docs/ARCHITECTURE.md` to "this document, its diagrams and the consolidation run logs (`docs/consolidations/`)", in the same commit.
- `git push origin HEAD:main`. If the push is rejected because `main` moved, rebase again, run the full gate again, correct the hashes in the log (`git commit --amend`) and push.
- No deploy and no waiting for CI: stkn.org installs the next release itself, and the user follows the CI run of the pushed commit.
- Afterwards remove the worktree (`git worktree remove`) if one was used and delete the local branch (`git branch -D`). If the run worked in `<repo>` itself, switch it back to `main` and `git pull --ff-only origin main` first. Stop the headless app and the test server by PID, and remove `/tmp/schwitzer-consolidate` and the Windows check directory.

## 8. Run log and report

The run log is the lasting record of a run; the next run reads it in step 3. File: `docs/consolidations/<YYYY-MM-DD>.md` (append `-2`, `-3` and so on if it exists; create the directory if needed), in English like the rest of the repository's documentation, with:

- Title, date, scope (whole repository or the argument), the `origin/main` commit the review started from, and whether the room was consulted (step 3).
- Target picture per reviewed area (step 5).
- Overview table `# | Finding | Kind | Compat | Result`, where `Result` is one of ✅ done (with commit hash), ❌ dropped, 🚫 declined, ⏸ open.
- Carried out: per commit, hash, subject and in one or two sentences what was unified and where it lives now.
- Dropped findings, each with its reason (wrong, not behavior-preserving, not finishable in one commit, not verifiable locally, taste, stylistic rewrite, breaks an invariant, fights an open task).
- Declined compat findings, with what they would break, so the next run does not ask again without a new reason.
- Open findings (⏸), each with the uncommitted file or the task in progress that held it back.
- Not carried out here: new dependencies and `deny.toml` exceptions, weida, iced_impeccable and omp changes, proposals to other Schwitzer projects (with the task number they got), release, site and deployment files, each with the next step.
- Greenfield observations too large or too open to act on, for example a different crate or module split, each with a one-line recommendation.
- Remaining debt and risks: what the run left deliberately, and where a consolidation commit carries a risk the checks may not have covered (macOS-only code, Windows code without `cargo.exe`, `ci.yml` changes, UI states no screenshot showed).
- Starting points for the next development phase: where the consolidated structure now makes the next feature cheap.

The run log is public. It may name a room task by number and title as the reason a finding was dropped or held back, but never quotes a task's body, plan or notes, and never quotes `SCHWITZER_PLAN.md`. Security observations never go into the run log or any other file in the repository: an unfixed weakness does not belong in a public document. They go only into the chat report.

A run with no finding carried out still writes a log, so that dropped and declined findings are recorded.

Report to the user in the chat: path of the run log, the target picture in brief, the pushed commits (hash and subject) grouped by `kind`, whether an approved `wire` or `stored` change will cut off older apps once the next release is published, the open points, remaining risks and starting points briefly, room tasks the code has already implemented or left (step 3), and the security observations in their own section (never fixed here).
