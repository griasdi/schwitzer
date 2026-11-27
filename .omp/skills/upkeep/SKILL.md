---
name: upkeep
description: Maintenance run for Schwitzer - update every dependency (lockfile and held-back majors), follow up every hold-back comment in Cargo.toml and every advisory cargo deny reports, move the iced_impeccable and weida pins and the skill copies that ride on them, refresh the pinned tools (rust-version, GitHub Actions, cargo-about with the license list, appimagetool and its runtime), and land each step as its own gated commit on main; moves that touch the wire protocol, stored formats, crypto, weida or a new iced minor only after the user approves them.
---

# upkeep

Goal: Schwitzer runs on the newest dependencies it can, carries no advisory, and every "held at" or "follows" comment is still true. A run ends with each constraint either lifted or re-confirmed with a current reason.

Upkeep moves versions; it is not a feature run and not a consolidation (that is the `consolidate` skill). What the app and the server do stays the same, except where a new version changes it and the commit says so.

This skill is in English; messages to the user are in the language the user writes in.

`AGENTS.md` and `CONTRIBUTING.md` apply in full: the gate, "Finishing a change", "Running headless" and the invariants. Most relevant here:

- An advisory is never silenced, a license never allowed and a source never added in `deny.toml` without the user's decision.
- A constraint that holds a crate below its newest release has a comment next to it in `Cargo.toml` saying why (as the `weida` and `pulldown-cmark` lines do). This run reads those comments, and every constraint it leaves in place keeps a true one.
- postcard is positional and the store maps enums to explicit integers: a dependency move must not change what goes over the wire or into `schwitzer.db`. One that would bumps `PROTOCOL_VERSION` or `SCHEMA_VERSION` and needs approval (step 6).
- stkn.org is never deployed, restarted or replaced, `deploy/deploy.sh` is never run, and no release is cut (`deploy/release.sh` is the user's). The release server installs the next release, the dev server (`schwitzer-server-dev`) the next `dev` build, by themselves.
- weida, iced_impeccable and omp are not changed from here. A change one of them needs is put to the user (step 6); griasdi, nuntio and spezl are Schwitzer projects and get a proposal instead (step 6).
- A security problem found on the way is reported to the user in the chat, never written into a file of the repository.
- `SCHWITZER_PLAN.md`, `.omp/mcp.json` and other local untracked files are never edited, deleted, moved or added to git.

## 1. Workspace

- Target repository: the main checkout (first `worktree` entry of `git worktree list --porcelain`) if it contains `crates/schwitzer-server/`, otherwise `~/code/schwitzer`. Below, `<repo>`.
- `git -C <repo> fetch -q origin`. Uncommitted changes to tracked files (`git status --porcelain --untracked-files=no`) or not on `main`: worktree `git -C <repo> worktree add <repo>/../schwitzer-upkeep -b upkeep-<YYYY-MM-DD> origin/main` (append `-2`, `-3` to path and branch if taken); its `target/` is cold, do not point `CARGO_TARGET_DIR` at the main checkout's. Otherwise branch `upkeep-<YYYY-MM-DD>` from `origin/main` in `<repo>`. The branch stays local and lands by rebase (step 8).
- Scratch space for the whole run: `/tmp/schwitzer-upkeep/` (headless home, server data, screenshots). Nothing reads or writes `~/.local/share/schwitzer`, port 13339 or stkn.org.
- Baseline on the starting commit: `cargo clippy --workspace --all-targets -- -D warnings`, the same with `--features schwitzer-app/remote`, `cargo test --workspace`, `cargo deny check`; if `cargo.exe` is on `PATH`, also the Windows check of step 7. What fails here is pre-existing: it goes into the report and is neither fixed nor blamed inside an upkeep commit, except an advisory, which this run is for.
- Open work in the room: `schwitzer_list_rooms`, then `schwitzer_list_tasks` for the project `schwitzer`, and `schwitzer_get_task` for every task in progress. A task whose plan names `Cargo.toml`, `Cargo.lock` or a crate it moves to a new version is asked about before a move touches the same crate (step 6). If the Schwitzer tools are unavailable (the user's app is not running), say so and continue.

## 2. Inventory (read-only)

Collect before changing anything:

- **Dependencies:** `cargo update --dry-run --verbose` lists the semver-compatible moves and, as `(available: …)`, newer incompatible versions (never `--workspace`: that moves only the workspace members). For every requirement in `[workspace.dependencies]` and in the crates' `Cargo.toml` note current, newest release and whether the step is compatible. What a release depends on: `curl -sA schwitzer-upkeep https://crates.io/api/v1/crates/<crate>/<version>/dependencies`.
- **Hold-backs and couplings:** every comment in the `Cargo.toml` files that keeps a crate below its newest release or ties it to another one (exact `=` pins, "follows", "the parser of iced's …", "which X already pulls in"), with the condition that would lift it, checked against today's releases. Known couplings: `pulldown-cmark` is the version iced's `markdown` widget uses (`logic::md_blocks` must split blocks the way the widget renders them); `resvg` renders the window icon (`brand.rs`) and the icon files (`examples/icons.rs`, with `ico` and `icns`) and stays on the series iced's `svg` feature uses, so the graph holds one copy; `iced_impeccable` is built for one iced series.
- **Pins outside Cargo:** `rust-version` in the root `Cargo.toml` and the version `CONTRIBUTING.md` names beside it; the actions in `.github/workflows/*.yml` (`actions/*`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `EmbarkStudios/cargo-deny-action`) against their newest major; `CARGO_ABOUT` in `crates/schwitzer-app/examples/licenses.rs` (also named in `AGENTS.md` and the example's error text) against the newest cargo-about; appimagetool and the type2 runtime in `deploy/package.sh` (version and sha256 per arch) against their newest releases (`gh release list -R AppImage/appimagetool`, `gh release view -R AppImage/type2-runtime`); the `iced-impeccable` version in `AGENTS.md` ("Running headless") and the `version:` of `.omp/skills/iced-impeccable/SKILL.md`; the `version:` of `.omp/skills/impeccable/SKILL.md` against the newest impeccable skill.
- **weida:** the exact pin against the newest release on crates.io (`curl -sA schwitzer-upkeep https://crates.io/api/v1/crates/weida`) and its changelog for the range (`~/code/weida`, fetched; never committed to).
- **iced_impeccable:** the requirement against the newest release on crates.io and the iced series that release builds on.
- **iced:** the newest 0.14.x and whether a newer minor exists (a migration, step 6).
- **Advisories:** `cargo deny check` for the current state: vulnerabilities, unmaintained direct dependencies, yanked crates, `multiple-versions` warnings (`cargo tree -d` shows which requirement pulls the older copy).
- **Earlier runs:** `git log --oneline -i -E --grep 'update|bump|upgrade|advisor|take .* which' -40`: what was moved or declined recently and why.

## 3. Lockfile

`cargo update`: compatible moves only, no `Cargo.toml` change. An exact `=` pin holds only that crate: weida's sub-crates (`weida-core`, `weida-protocol`, `weida-runtime`) follow their own requirements; they stay on the versions the pinned weida release was published with (its crates.io dependency list), moved with `--precise` if `cargo update` lifted them.

`cargo run -p schwitzer-app --example licenses` afterwards (needs the cargo-about version of `CARGO_ABOUT`; its test fails whenever `Cargo.lock` moved), and `assets/licenses/third-party.json` goes into the same commit.

## 4. Requirements

In this order, each its own commit:

1. **Hold-backs** whose lifting condition is met: requirement raised, comment removed or rewritten; constraints still needed keep a comment with the current reason (newest version named, what blocks it).
2. **Incompatible majors** without a hold-back, one crate or one coupled group per commit (`iced` with `pulldown-cmark`, `resvg` and `iced_impeccable`; `axum` with `rmcp` when rmcp's transport needs it; `ureq` with the rustls it brings; `rand` with whatever shares its `rand_core`). Read the crate's changelog for the range first and adapt the code with a clean cutover, no compatibility shims. A move that brings a second copy of a crate into the graph is not made unless the other copy's user moves in the same commit; otherwise it becomes a hold-back with its comment.
3. **iced_impeccable** to its newest release on Schwitzer's iced series: requirement raised, `cargo update -p iced_impeccable`, the version in `AGENTS.md` ("Running headless") raised, and the skill copy refreshed with the new binary (`cargo install iced_impeccable --version <v> --locked`, then `iced-impeccable install --agent omp --yes .`), all in one commit. Headless check (step 7) through the new `ctl`.
4. **impeccable skill:** to its newest release when the `version:` moved, with impeccable's own CLI (`npx impeccable --help` shows how it installs into a repository); its own commit, without a Changelog trailer.

Every requirement change is followed by the license example of step 3 when `Cargo.lock` moved.

A move that does not build cleanly within one commit is dropped (`git reset --hard HEAD`) and reported with the blocker, never landed half-way.

## 5. Tools and toolchain

Each its own commit, every place that names the version in the same commit:

- `rust-version`: raised only when a dependency of this run needs a newer Rust, to what it needs; `CONTRIBUTING.md` names the same number.
- GitHub Actions: each action at its newest major in all three workflows. `site.yml` and `release.yml` run only on the site deploy, a tag or a dev build: read the action's changelog for removed inputs the workflow uses, and the report names which run will exercise them.
- cargo-about: `CARGO_ABOUT` and the install hint in `licenses.rs`, `AGENTS.md` and `CONTRIBUTING.md` where they name it; `cargo install --locked --features cli cargo-about@<v>`, then the license example. A new version may pick other license texts: read the diff of `third-party.json`, and a license that leaves the allow lists of `deny.toml` and `crates/schwitzer-app/about.toml` is the user's decision.
- appimagetool and the type2 runtime: URL, version in the file names and the sha256 of both arches in `deploy/package.sh`. Compute the sha256 from the downloaded asset and compare it with the digest GitHub lists for it (`gh release view … --json assets`); a mismatch stops this move and goes into the report.

## 6. Approval

After the inventory, one `ask` call with one checkbox option per move that needs the user, each with changelog highlights and what breaks:

- `weida`: exact pin, alpha, its wire protocol may break within 0.x; a move cuts off apps older than the next release from a server that runs it, stkn.org among them. Name the weida changelog entries of the range.
- Anything that changes the wire or stored format: `postcard` (the encoding of every frame), `serde` behavior the protocol relies on, `rusqlite` majors (the bundled SQLite and `schwitzer.db`), `toml` majors (how `settings.toml` is read), and every move that needs a `PROTOCOL_VERSION` or `SCHEMA_VERSION` bump.
- Crypto and identity: `sha2`, `blake3`, `rand` majors, and anything that changes TLS (rustls through `ureq` or weida), the QUIC key fingerprint or the `sha256:` digest checks of `schwitzer-release`.
- `rmcp` and `axum` majors: the MCP server the omp plugin talks to (`mcp/serve.rs`, the bearer and `Origin` checks).
- A new iced minor: a migration of the app and of iced_impeccable, not upkeep. It goes to the user as its own proposal with an estimate of what breaks; it waits for an iced_impeccable release on that series.
- A crate added to the graph that is not a new version of an existing one, and every new `deny.toml` exception.
- A move touching a crate the plan of a task in progress names (step 1), with the task number.

Declined moves stay as they are; their hold-back comment names the decision.

A change weida or iced_impeccable needs (a fix, a release for a new iced): they are not Schwitzer projects, so it is described to the user and left to them. A change griasdi, nuntio or spezl needs: draft a proposal for that project, a one-line title and a Markdown body starting with `Requested from project "schwitzer" in room "<room name>"`; show title and body in the chat, ask with `ask` naming only the title whether to send, change or drop it, and call `schwitzer_propose` only after the user agrees.

## 7. Per commit

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, the same with `--features schwitzer-app/remote`, `cargo test` for the crates the change touches (lockfile and widely used crates: `--workspace`). A move of weida, quinn, rustls, tokio, postcard or rusqlite: `cargo test -p schwitzer-client` too (its flow test runs a real server on loopback and restarts it on the same data dir). A changed doc comment: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
- `cargo deny check` after every change to the graph.
- Windows: if the move touches a crate with Windows-specific code paths (`zip` in `schwitzer-release`, `winresource` in `build.rs`, `rfd`, `notify-rust`, `directories`) or the lockfile as a whole, and `cargo.exe` is on `PATH`: `win="$(wslpath "$(cmd.exe /c 'echo %LOCALAPPDATA%' 2>/dev/null | tr -d '\r')")/Temp/schwitzer-upkeep"`, `mkdir -p "$win"`, `find "$win" -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +`, `git archive HEAD | tar -x -C "$win"`, then in `$win` `cargo.exe clippy -p schwitzer-app -p schwitzer-release --all-targets --features schwitzer-app/self-update -- -D warnings` and `cargo.exe test -p schwitzer-release`. Without `cargo.exe` the report says it is left to CI.
- macOS (`icns`, `target_os = "macos"` code) is not compiled here; CI checks it after the push. Such moves go into the report.
- A move that touches what the window draws (iced, resvg, pulldown-cmark, iced_impeccable, rfd, notify-rust): check headless as in `AGENTS.md` "Running headless", once per run set up as `mkdir -p /tmp/schwitzer-upkeep/{a,omp,server,shots}`, `printf 'name = "upkeep"\nmcp_port = 13471\nomp_sessions_dir = "/tmp/schwitzer-upkeep/omp"\n' >/tmp/schwitzer-upkeep/a/settings.toml`, the server from `target/` on `127.0.0.1:7474` with `--data-dir /tmp/schwitzer-upkeep/server`. Screenshot the list, an open task with Markdown details (headings, a list, code, a link) and the Settings overview before the move and after it; they look the same, and the Markdown blocks still carry their comment buttons (`logic::md_blocks` agrees with the widget). Processes are stopped by PID.
- A move of `rmcp` or `axum`: start the headless app and call one MCP tool over `http://127.0.0.1:13471/mcp` with the bearer from `/tmp/schwitzer-upkeep/a/mcp-token`; without the bearer it answers 401, with an `Origin` header 403.
- Commit per `CONTRIBUTING.md`: imperative subject without prefix, at most 72 characters; the body says what moved and why (which condition was met, which advisory it fixes, what was confirmed). Stage paths explicitly. Trailer: `Changelog: security` when the move fixes an advisory, `Changelog: changed` or `fixed` when users notice it, none for an internal move.

## 8. Land and report

- Full gate of `AGENTS.md`: `cargo fmt --all --check`, both clippy runs, `cargo test --workspace`, `cargo deny check`, and `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`. If any commit triggered the Windows check, that check once more on `HEAD`.
- `git pull --rebase origin main`; new commits mean the gate again, and a moved `Cargo.lock` the license example again. `git push origin HEAD:main`; rejected: rebase, gate, push.
- If the run changed a workflow or `deploy/package.sh`: `gh run watch` the CI run of the pushed commit and the `release.yml` run that builds the `dev` prerelease from it. A failure there is fixed in a follow-up commit through the same gate; the report names it.
- Remove the worktree (`git worktree remove`) and the local `upkeep-*` branch; if the run worked in `<repo>` itself, switch back to `main` and `git pull --ff-only origin main` first. Stop the headless app and the test server by PID, remove `/tmp/schwitzer-upkeep` and the Windows check directory.

Report in the chat: the pushed commits (hash, subject); per dependency group what moved; the hold-backs still in place and what they wait for; the advisories fixed and any left, with their options; moves asked and declined; whether a move changes the protocol or schema and so cuts off older apps once the next release is published; what CI, a dev build or a tag will exercise first (workflows, `package.sh`, macOS, Windows without `cargo.exe`); that the changes reach users with the next release, which the user cuts; proposals drafted for other projects; and security observations in their own section.
