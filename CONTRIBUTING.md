# Contributing to Schwitzer

This document explains how to build and test Schwitzer and how to commit changes.

## Prerequisites

- Stable Rust, at least the `rust-version` in `Cargo.toml` (1.91), with rustfmt and clippy.
- A C compiler: `rusqlite` builds SQLite from source (`bundled`).
- `cargo-deny` (`cargo install --locked cargo-deny`) for the dependency check.

## Build & run

```sh
cargo run -p schwitzer-server -- --listen 0.0.0.0:7444 --public-host <name clients dial> --data-dir <dir>
cargo run -p schwitzer-app                                 # the desktop app (binary `schwitzer`)
cargo build --workspace --features schwitzer-app/remote    # app with headless remote control (iced_impeccable)
RUST_LOG=debug cargo run -p schwitzer-app                  # verbose logging, to stderr
```

The server prints its URL (`server url: weida://sha256:…@host:port`) on stdout; that URL goes into the app's "Create room" dialog. The app keeps its data in the platform's data directory (`~/.local/share/schwitzer` on Linux), or in `$SCHWITZER_HOME` if set.

`deploy/deploy.sh stkn` (Linux, systemd) builds the committed `HEAD` in release mode on this machine and deploys it over ssh (`stkn.org`, as your ssh user) as the systemd user service `schwitzer-server` on UDP 7444 (IPv4 and IPv6) with public host `stkn.org` and its data in `~/.local/share/schwitzer-server` there. It restarts the service only when the binary or unit changed and enables lingering so the service runs without a login. The target needs `rsync` and the same or a newer glibc than the build machine. `deploy/release.sh` runs it after pushing the release tag: stkn.org runs the latest release, not every push to `main`.

## Before pushing

GitHub Actions (`.github/workflows/ci.yml`) runs these checks on every push to `main` and every pull request, and app clippy and the updater tests on macOS and Windows. Run them before every commit that touches code all the same:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features schwitzer-app/remote -- -D warnings
cargo test --workspace
cargo deny check
```

For UI changes, also click through the affected flow in Daylight and Night (DESIGN.md is the reference) at the default window size of 400×780.

## Layout

| Crate | Contents |
|---|---|
| `crates/schwitzer-proto` | Wire types, ids, framing, room keys |
| `crates/schwitzer-server` | Binary `schwitzer-server`: rooms in SQLite, RPC and event stream over weida (QUIC) |
| `crates/schwitzer-client` | Client library: connection, reconnect, room state |
| `crates/schwitzer-app` | Binary `schwitzer`: iced desktop app, MCP server for the omp plugin, omp session sharing |

Conventions:
- Errors: `thiserror` in the libraries (`schwitzer-proto`, `schwitzer-client`), `anyhow` in `schwitzer-server`.
- Log with `tracing`, not `println!`; stdout is reserved for the server URL and `ctl` replies.
- postcard is positional: enum variants on the wire are only ever appended; an incompatible change bumps `PROTOCOL_VERSION`.
- Keep pure logic (`app/logic.rs`, the store, the protocol) free of iced types so it can be unit tested.

## Releases

```sh
deploy/release.sh 0.2.0
```

Run it on `main` with `HEAD` pushed and no uncommitted changes to tracked files. It sets the workspace version (a `Release 0.2.0` commit when it changes), prints the release notes and pushes the tag `v0.2.0`. The tag starts `.github/workflows/release.yml`, which builds the app with `--features self-update` for linux-x86_64, linux-aarch64, macos-aarch64 and windows-x86_64, packs it with `deploy/package.sh` and publishes a GitHub release with notes from `deploy/changelog.sh`. Then it runs `deploy/deploy.sh stkn`, so stkn.org runs the released server. The server switches before the release workflow has published the app: for the length of that build, a protocol change leaves older apps without an update to install.

Each server checks the GitHub releases every 5 minutes and announces the newest stable one, whatever its own version. Release builds install the highest version their online servers announced, or GitHub's latest release when no online server announced one, fetched and verified from GitHub, and restart into it, back on the page and draft they showed (Settings → Updates switches this off). Builds without `self-update` never update themselves. `SCHWITZER_UPDATE_URL` replaces the GitHub API URL of the release list, for a local test feed; `schwitzer-server --releases-url` replaces the server's.

## Commits & changelog

The changelog is generated from **commit trailers**. There is no hand-maintained CHANGELOG file.

### Commit message

```
Short summary in imperative mood (≤ 72 chars)

Optional body: what and why, not how.
Wrap lines at ~72 characters.

Changelog: fixed
```

- **No prefixes.** The subject starts with the verb: no `feat:`, `fix:`, `chore:`, no scope like `server:` or `app:`, no `Schwitzer:`. The trailer carries the category, the diff carries the scope.
- The **subject line becomes the changelog entry**. Write it so users understand it.
- The `Changelog:` trailer goes in the last paragraph of the message, separated by a blank line.
- At most one `Changelog:` trailer per commit.

### Categories

| Trailer | Use for |
|---|---|
| `Changelog: added` | New feature |
| `Changelog: changed` | Changed behavior of an existing feature |
| `Changelog: deprecated` | Feature scheduled for removal |
| `Changelog: removed` | Feature removed |
| `Changelog: fixed` | Bug fix |
| `Changelog: security` | Security-relevant fix |
| `Changelog: performance` | Noticeable performance improvement |

**No trailer** means the commit is left out of the changelog. That's right for refactorings, tests, internal docs and anything users won't notice.

### Examples

```
Show the claiming member's tag on in-progress tasks

Changelog: added
```

```
Reconnect after the server restarts on the same address

Changelog: fixed
```

```
Move the pick prompt builder into app/logic.rs
```

Not:

```
feat(app): show member tag
fix: reconnect
```

### Release notes

```sh
deploy/changelog.sh v0.1.0..HEAD
```

This prints Markdown release notes, one section per category, oldest commit first; leave out the range for the whole history.

### Pull requests

- Keep a PR to one topic. Multiple commits are fine as long as each one makes sense on its own.
- If a PR is squash-merged, the `Changelog:` trailer must be kept in the final squash message, and the subject stays without a prefix.

## License

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Schwitzer by you, as defined in the Apache-2.0 license, shall be dual licensed under the MIT license and the Apache License 2.0 (`MIT OR Apache-2.0`), without any additional terms or conditions.
