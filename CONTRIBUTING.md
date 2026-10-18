# Contributing to Schwitzer

This document explains how to build and test Schwitzer and how to commit changes.

## Prerequisites

- Stable Rust, at least the `rust-version` in `Cargo.toml` (1.91), with rustfmt and clippy.
- A C compiler: `rusqlite` builds SQLite from source (`bundled`).

## Build & run

```sh
cargo run -p schwitzer-server -- --listen 0.0.0.0:7444 --public-host <name clients dial> --data-dir <dir>
cargo run -p schwitzer-app                                 # the desktop app (binary `schwitzer`)
cargo build --workspace --features schwitzer-app/remote    # app with headless remote control (iced_impeccable)
RUST_LOG=debug cargo run -p schwitzer-app                  # verbose logging, to stderr
```

The server prints its URL (`server url: weida://sha256:…@host:port`) on stdout; that URL goes into the app's "Create room" dialog. The app keeps its data in the platform's data directory (`~/.local/share/schwitzer` on Linux), or in `$SCHWITZER_HOME` if set.

`deploy/deploy.sh local` (Linux, systemd) builds the committed `HEAD` and runs it as the systemd user service `schwitzer-server-local` on `127.0.0.1:7444`, with its data in `~/.local/share/schwitzer-server`; it restarts the service only when the binary or unit changed.

`deploy/deploy.sh stkn` builds the same way on this machine and deploys over ssh (`felix@stkn.org`) as the systemd user service `schwitzer-server` on UDP 7444 (IPv4 and IPv6) with public host `stkn.org`; it enables lingering there so the service runs without a login. The target needs `rsync` and the same or a newer glibc than the build machine.

## Before pushing

There is no CI: these checks are the only ones, run them before every commit that touches code:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features schwitzer-app/remote -- -D warnings
cargo test --workspace
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

### Listing changelog entries

```sh
git log --grep='^Changelog:' --format='%(trailers:key=Changelog,valueonly,separator=%x2C)%x09%s' <from>..HEAD | sort
```

This prints one line per entry, category then subject; leave out `<from>..HEAD` for the whole history.

### Pull requests

- Keep a PR to one topic. Multiple commits are fine as long as each one makes sense on its own.
- If a PR is squash-merged, the `Changelog:` trailer must be kept in the final squash message, and the subject stays without a prefix.
