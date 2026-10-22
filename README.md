# Schwitzer

A task room for coding agents and their humans: someone opens a room without signing up and shares its key. Humans or agents note ideas and propose tasks; humans accept them; agents write plans; humans approve; agents build them one by one, each claiming the next free task. Schwitzer is a narrow desktop app (Linux, Windows, macOS) that sits beside the editor; rooms live on a standalone `schwitzer-server`.

## How it works

- Stages: Idea → Proposed → Accepted → Planned → Ready → In progress → Done (or Rejected).
- Humans decide: accept, reject, approve, replan and reopen exist only as buttons in the app, never as agent tools.
- omp agents work through the omp plugin the app installs into linked repositories:
  - `/schwitzer plan`: pick an accepted, unclaimed task, claim it for planning, start plan mode.
  - `/schwitzer build`: pick an approved (`ready`), unclaimed task, claim it for building.
  - `/schwitzer propose [text]`: the agent drafts a proposal (or writes up an open idea), asks, then sends it.
  - `/schwitzer idea <text>`: note an idea in a project right away, without an agent turn.

  Details in [the plugin's README](crates/schwitzer-app/omp-plugin/README.md).
- Live agent sessions: omp sessions whose working directory lies in a repository linked to a room's project are shared with the room (title, project, runtime, tokens). Other sessions never leave the machine.

## Install

Download the [latest release](https://github.com/griasdi/schwitzer/releases/latest):

- Linux: download the `.AppImage` for x86_64 or aarch64, make it executable (`chmod +x`) and run it. Or unpack the `.tar.gz` and run `schwitzer`.
- macOS (Apple silicon): unpack, move `Schwitzer.app` to Applications and run `xattr -dr com.apple.quarantine /Applications/Schwitzer.app` once; the app is not notarized.
- Windows: run `schwitzer-<version>-windows-x86_64-setup.exe`; it installs for your user, without admin rights. Or unpack the zip and run `schwitzer.exe`. SmartScreen asks once because the app is not signed.

Keep the app (the AppImage too) in a folder you can write to: it installs the newest release your servers announce, or GitHub's latest one when no server is reachable, in place and restarts. Settings → Updates switches this off.

## Getting started

1. Open Settings → "All rooms" (or "Join or create a room" when you are in none). Under "Create a room" enter a server URL (`weida://sha256:…@host:7444`) and a room name, then press "Create". Or under "Join a room" paste a room key (`schwitzer://sha256:…/<key>`) someone shared and press "Join".
2. Share the room key with the others in the room: the invite icon in the header bar copies it ("Room key copied"), as does "Copy invite key" on the room's settings page.
3. In Settings, open the project's page and link your local repositories under "Repos on this machine": press "Link" at a suggested folder, or enter a path and press "Link". The app installs the omp plugin there, and a project's tasks show once you have linked a repo to it.
4. In such a repo, start omp and run `/schwitzer`.

## Run your own server

On Linux (x86_64 or aarch64), download `schwitzer-server-<version>-linux-<arch>.tar.gz` from the [releases](https://github.com/griasdi/schwitzer/releases) and unpack it; the binary is static. Or build it with `cargo build --release -p schwitzer-server`. Then:

```sh
./schwitzer-server --listen 0.0.0.0:7444 --public-host <name clients dial> --data-dir <dir> --auto-update
```

The server listens on UDP (QUIC) and prints `server url: weida://sha256:…@host:port` on stdout; that URL goes into "Create a room". `--public-host` must be the name clients dial. Always pass `--data-dir` (it defaults to `./schwitzer-data` in the working directory). With `--auto-update` the server installs each new stable release over its own program file and restarts into it in place; keep the binary in a folder the server's user can write to. [`deploy/schwitzer-server.service`](deploy/schwitzer-server.service) is an example systemd unit.

## Build from source

See [CONTRIBUTING.md](CONTRIBUTING.md) for prerequisites, building, checks, commit rules and releases. [PRODUCT.md](PRODUCT.md) says what Schwitzer is for, [DESIGN.md](DESIGN.md) how it looks, and [AGENTS.md](AGENTS.md) is for coding agents working on this repo. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) shows how the pieces fit together and how agents work through the omp plugin.

## License

Schwitzer is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. The app lists the licenses of the open source components it is built from under Settings → About → Licenses.
