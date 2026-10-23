# Schwitzer

A task room for coding agents and their humans: someone opens a room without signing up and shares its key. Humans or agents note ideas and propose tasks; humans accept them; agents write plans; humans approve; agents build them one by one, each claiming the next free task. Schwitzer is a narrow desktop app (Linux, Windows, macOS) that sits beside the editor; rooms live on a standalone `schwitzer-server`.

[Website](https://griasdi.github.io/schwitzer/) · [Download](https://github.com/griasdi/schwitzer/releases/latest) · [Architecture](docs/ARCHITECTURE.md) · [omp plugin](crates/schwitzer-app/omp-plugin/README.md)

## Contents

- [How it works](#how-it-works)
- [Install](#install)
- [Getting started](#getting-started)
- [Using the app](#using-the-app)
- [Settings and data](#settings-and-data)
- [Updates](#updates)
- [Windows and WSL](#windows-and-wsl)
- [Run your own server](#run-your-own-server)
- [Build from source](#build-from-source)
- [License](#license)

## How it works

### Rooms, projects and tasks

There are no accounts. Your identity is a key the app creates on your machine, and a member is that key.

A room lives on a server. Whoever holds its room key (`schwitzer://sha256:<server fingerprint>@host:port/<token>`) can join it and sees all its tasks.

A room has projects. Each task belongs to one project and has a number per room, a title, a body, a plan, a priority (High, Normal or Low), notes, and optionally tasks it waits for. A task that waits for others cannot be picked for building until they are Done.

### Stages

|Stage|How it gets there|Shown in the app as|
|---|---|---|
|Idea|noted by a human or an agent|"Idea"|
|Proposed|a proposal, or an idea written up|"Proposed"|
|Accepted|a human pressed "Accept"|"Accepted", or "Being planned" while an agent holds a plan claim|
|Planned|the agent submitted a plan, or a human pressed "Mark planned"|"Planned", or "Plan ready" once a plan exists|
|Ready|a human pressed "Approve & build"|"Approved", or "Building" while claimed|
|Done|the agent finished, or a human pressed "Mark done"|"Done"|
|Rejected|a human pressed "Reject"|"Rejected"|

Humans decide: accept, reject, approve, replan, reopen, mark planned and mark done exist only as buttons in the app, never as agent tools.

An agent claims one task at a time. Claims never expire; anyone in the room can press "Unclaim" to free a stuck one. The full state machine is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#4-rooms-projects-and-tasks).

### Agents

omp agents work through the omp plugin the app installs into linked repositories:

- `/schwitzer plan`: pick an accepted, unclaimed task, claim it for planning, start plan mode.
- `/schwitzer build`: pick an approved (`ready`), unclaimed task, claim it for building.
- `/schwitzer build new`: open a fresh session with `/schwitzer build` in its editor.
- `/schwitzer propose [text]`: the agent drafts a proposal (or writes up an open idea), asks, then sends it.
- `/schwitzer idea <text>`: note an idea in a project right away, without an agent turn.
- `/schwitzer debug [stop]`: start or join the room's debug session, or leave it (see below).

The plugin also gives the agent these tools: `schwitzer_list_rooms`, `schwitzer_list_tasks`, `schwitzer_get_task`, `schwitzer_propose`, `schwitzer_add_idea`, `schwitzer_claim`, `schwitzer_submit_plan`, `schwitzer_finish`, `schwitzer_unclaim`, `schwitzer_add_note`, `schwitzer_debug_status`, `schwitzer_debug_instruct`.

The app writes the plugin into a linked repo under `.omp/plugins/` (`node_modules/schwitzer/{index.ts,package.json,schwitzer.json}` and `omp-plugins.lock.json`). The app does not edit `.gitignore`, so these files show up as untracked until you ignore them.

Linking a repo does not install the plugin: press "Install" on the project page. Out-of-date plugins are rewritten automatically while "Update automatically" (omp settings page) is on; omp loads the new version on its next start.

The plugin talks to the running app at `http://127.0.0.1:<mcp_port>/mcp` (default port 13339) and authenticates with the token in `<data dir>/mcp-token`. That endpoint accepts loopback connections only and refuses browser requests. When the app is not running, tool calls fail with "not reachable at <url>; is the app running?".

"Task number in commits" (project page, "Your agents") makes omp hold back a `git commit` during a build claim until the message carries `Schwitzer-Task: #<number>`.

Details in [the plugin's README](crates/schwitzer-app/omp-plugin/README.md).

### Debug sessions

Members test a networked app together, for example a stream from one machine to another, with omp and Schwitzer running on each machine. One member starts a debug session in a room (the bug button in the header, or `/schwitzer debug`) and becomes its master; others join it the same way.

- Every participant's app streams the files in its `out` folder (`<data dir>/debug/<session>/out`) to all other participants, whose apps write them to `in/<member>-<id>/`. Write a log there, for example `your-app 2>&1 | tee <out>/app.log`. Who joins late gets only what is written after.
- The master's agent sends instructions with `schwitzer_debug_instruct`. On a participant's machine an instruction appears as a dialog in the omp session that ran `/schwitzer debug`; only "Put it into the editor" places it there, and the human presses Enter. Nothing runs by itself.
- The server relays logs and instructions in memory while the session runs and stores nothing. The master leaving ends the session for everyone.

### Live sessions and privacy

- Only omp sessions whose working directory lies in a repository linked to one of the room's projects are shared with the room.
- What is shared: title, project, model, start and last activity time, token count, and the task it works on. A session counts as active while its files changed within the last 10 minutes, and shows "idle" after 2 minutes without a write.
- A session is tied to a task by the `schwitzer-claim: <token>` line from the claim.
- Other sessions, your code and the MCP token never leave the machine.
- The server keeps shared sessions in memory only.
- The server stores tasks, plans, notes, and room, project and member names in plain text. TLS protects them only on the wire. Run your own server if that matters.

## Install

Download the [latest release](https://github.com/griasdi/schwitzer/releases/latest):

- Linux: download the `.AppImage` for x86_64 or aarch64, make it executable (`chmod +x`) and run it. Or unpack the `.tar.gz` and run `schwitzer`.
- macOS (Apple silicon): unpack, move `Schwitzer.app` to Applications and run `xattr -dr com.apple.quarantine /Applications/Schwitzer.app` once; the app is not notarized.
- Windows: run `schwitzer-<version>-windows-x86_64-setup.exe`; it installs for your user, without admin rights. Or unpack the zip and run `schwitzer.exe`. SmartScreen asks once because the app is not signed.

The app updates itself; see [Updates](#updates).

## Getting started

Agents need omp.

0. On the first start, the "Your repos" page asks for the folders that hold your repos. It pre-fills the existing ones of `~/code`, `~/projects` and `~/work`. Repos in them whose name matches a project, or contains it, are suggested under "Suggested" when you link.
1. Open Settings → "All rooms" (or "Join or create a room" when you are in none). Under "Create a room" enter a server URL (`weida://sha256:…@host:7444`) and a room name, then press "Create". To try it, use the public server `weida://sha256:b4e7d073f1bc6e0c663202055c74bfbd703aa83eadc79d58c55d5d9934b3e4cc@stkn.org:7444`. It stores your room's tasks; [run your own](#run-your-own-server) if you'd rather keep them. Or under "Join a room" paste a room key (`schwitzer://sha256:…/<key>`) someone shared and press "Join".
2. Share the room key with the others in the room: the invite icon in the header bar copies it ("Room key copied"), as does "Copy invite key" on the room's settings page.
3. In Settings, open the project's page and link your local repositories under "Repos on this machine": press "Link" at a suggested folder, or enter a path and press "Link". Then press "Install" to put the omp plugin there. A project's tasks show once you have linked a repo to it.
4. In such a repo, start omp and run `/schwitzer`.

## Using the app

The header bar holds the room picker, the project picker ("New project…" at the end), the members button (`online/total`), the invite icon "Copy room key to invite someone", and Settings.

The task list groups tasks in this order: In progress, Ready to pick, Planning, Proposals, Ideas, Done. Each task shows one next-step button:

- "Accept" on a proposal.
- "Approve" on a task whose plan is ready.
- "Pick" on an approved task: it claims the build and copies a prompt for your agent. It is disabled while the task waits for open tasks.
- "Unclaim" on your own claim.

An opened task lets you:

- edit the title and details, and the plan (Markdown);
- set the priority (High, Normal, Low);
- move it to another project with the project picker, shown when the room has more than one project;
- add or remove tasks under "Waits for";
- add notes;
- press the decision buttons its stage allows.

The sessions panel shows the live agent sessions of the open project. The status bar shows your name and one status lamp for the server, the agent port and the omp plugin; hover it to see each.

|Shortcut|Action|
|---|---|
|`Ctrl+N` (`Cmd+N` on macOS)|new proposal|
|`Ctrl+,`|Settings|
|`Esc`|cancel a confirm, close the open task, or go back|
|`Tab` / `Shift+Tab`|move focus|

Leaving a room: room page → "Leave room". When the server does not answer, it offers "Remove from this app" instead. That forgets the room locally only; the server keeps you as a member.

Projects: create one on the room page ("New project name"), rename it on the project page. "Delete project" works only for a project without tasks, and removes it for everyone.

"Notify when my agents hand in" (Settings, "You") shows a system notification when one of your agents hands in a plan or finishes a build.

## Settings and data

The data directory is:

- Linux: `~/.local/share/schwitzer`, or `$XDG_DATA_HOME/schwitzer`.
- macOS: `~/Library/Application Support/schwitzer`.
- Windows: `%APPDATA%\schwitzer\data`.

`SCHWITZER_HOME` replaces it. It holds:

- `identity/`: your key. Losing it makes you a new member in every room.
- `settings.toml`: the settings below.
- `mcp-token`: the agents' token, mode 0600; the app replaces it if others can read it.
- `resume.json`: written only for an update restart.

|Key|Type|Default|Meaning|
|---|---|---|---|
|`name`|string|`$USER` / `$USERNAME`, else `anon`|display name shown to the other members|
|`mcp_port`|integer, 1–65535|`13339`|port of the agents' endpoint on `127.0.0.1`|
|`omp_sessions_dir`|path|`~/.omp/agent/sessions`|where omp keeps its session files|
|`rooms`|list of room keys|empty|your rooms, in list order|
|`last_server`|server URL|unset|prefills "Create a room"|
|`repos`|table of path lists|empty|linked repos per project, keyed `"<room id hex>/<project id>"`|
|`chosen_projects`|table|empty|the project last picked per room|
|`commit_trailers`|list|empty|projects (keyed like `repos`) with "Task number in commits" on|
|`projects_dirs`|list of paths|filled on first start|folders searched for repos to suggest|
|`wsl_distros`|list|empty|WSL distros whose folders were added to `projects_dirs` once|
|`auto_update_plugins`|bool|`true`|rewrite out-of-date omp plugins automatically|
|`update_channel`|`"release"` or `"dev"`|the build's own (dev for a dev build)|update channel; set in Settings → Updates|
|`auto_install_updates`|bool|`true`|install a new release as soon as it is found|
|`auto_restart_updates`|bool|`true`|restart after installing an update|
|`notify_hand_ins`|bool|`false`|notify when your agents hand in a plan or finish a build|

The Settings pages edit all of these. A `settings.toml` that cannot be read loads as defaults with a notice, and is not overwritten until the next save.

Environment variables:

- `SCHWITZER_HOME`: the data directory.
- `RUST_LOG`: log filter, default `info,iced_winit=warn`. Logs go to stderr; there is no log file.
- `SCHWITZER_UPDATE_URL`: replaces the GitHub releases API URL, for a test feed.

## Updates

Release builds update themselves. Every 5 minutes they check for the highest version announced by the online servers of your rooms, or GitHub's latest release when no server announced one (the Release channel). On the Dev channel they instead take the newest build of GitHub's rolling `dev` prerelease, built from every commit on `main` that passed CI, and ignore announcements. Each download is checked against the sha256 digest GitHub lists. The app then swaps its own program file (the `.AppImage` itself when it runs from one), restarts, and reopens the page, task and unsaved text it showed.

Settings → Updates has the channel (Release or Dev; `update_channel` in `settings.toml`, a dev build starts on Dev), "Check now", "Install" and "Restart now", and the switches "Install updates" and "Restart after installing". Switching the channel installs the newest build of the new channel at once, even when it is older: going from Dev back to Release is a downgrade. There is no version picker.

Keep the app (the AppImage too) in a folder you can write to. Builds from source without `--features self-update` never update.

## Windows and WSL

On Windows the app also reads the omp sessions of running WSL distros (`~/.omp/agent/sessions` in each home). It accepts repos inside a distro (`\\wsl.localhost\<distro>\…`), and adds each distro's `~/code`, `~/projects` and `~/work` to the projects folders once. A stopped distro is never started or read; its sessions show once it runs. The plugin in a WSL repo gets Linux paths.

## Run your own server

The server is one binary that hosts rooms over QUIC (UDP). It needs no accounts and has no admin: anyone who can reach it and knows its URL can create rooms. Joining a room needs its room key.

### Get it

- Download `schwitzer-server-<version>-linux-<arch>.tar.gz` (x86_64 or aarch64) from the [releases](https://github.com/griasdi/schwitzer/releases) and unpack it. It holds the single static (musl) binary `schwitzer-server`.
- Or build it with `cargo build --release -p schwitzer-server`, on any platform Rust supports. `--auto-update` works only on Linux x86_64 and aarch64.

### Start it

```sh
./schwitzer-server --listen 0.0.0.0:7444 --public-host <name clients dial> --data-dir <dir> --auto-update
```

stdout carries the single line `server url: weida://sha256:<fingerprint>@<public-host>:<port>`. That URL goes into "Create a room". Logs go to stderr.

|Flag|Default|Meaning|
|---|---|---|
|`--listen`|`0.0.0.0:7444`|UDP address; `[::]:7444` serves IPv6 and, with Linux's default `net.ipv6.bindv6only=0`, IPv4 too; port `0` picks a free one and the URL shows it|
|`--public-host`|`127.0.0.1`|name clients dial; goes into the URL|
|`--data-dir`|`./schwitzer-data`|database and identity; always pass it|
|`--releases-url`|`https://api.github.com/repos/griasdi/schwitzer/releases`|GitHub API release list the server checks|
|`--auto-update`|off|install new releases of the `--update-channel` and restart into them|
|`--update-channel`|`release`|with `--auto-update`: `release` installs new stable releases, `dev` every build of the rolling `dev` prerelease|
|`--version`, `--help`||`--version` prints the build's version (`x.y.z-dev.n` for a dev build)|

### Network

- Open inbound UDP on the listen port. There is no TCP and no HTTP listener.
- Clients send keep-alives every 10 s, and a connection is dropped after 30 s without traffic. That keeps NAT mappings alive from the client side.
- The release check and `--auto-update` need outbound HTTPS to GitHub (`api.github.com` and the release download hosts). Without it the server works, logs a warning every 2 minutes, and announces nothing.

### Identity and the server URL

- `<data-dir>/identity/` holds `key.pem` and `cert.pem`: directory 0700, files 0600. They are created on first start.
- The fingerprint in the URL is the key's. Apps trust only the key whose fingerprint they dial; no certificate authority is involved.
- Changing `--public-host` later keeps the fingerprint but changes the URL host. Existing room keys still carry the old name, so keep that name resolving to the server.
- Losing or replacing the key changes the fingerprint. Every saved server URL and room key then stops working, although the rooms stay in the database. Back the identity up with the database.
- If only one of the two files exists, the server refuses to start with "one of cert.pem and key.pem exists without the other".

### Data and backups

`<data-dir>/schwitzer.db` is SQLite in WAL mode, with `schwitzer.db-wal` and `schwitzer.db-shm` beside it, plus `identity/`.

- Back up while the server runs:

  ```sh
  sqlite3 <data-dir>/schwitzer.db ".backup '/backup/schwitzer.db'"
  ```

  Or stop the server and copy `schwitzer.db*` and `identity/` together. Never copy only `schwitzer.db` from a running server: recent changes may still be in the `-wal` file.
- Restore: stop the server, put back `schwitzer.db` and `identity/`, remove stale `-wal` and `-shm` files, then start it.
- Older database schemas are migrated on start (logged as "migrated the database from schema version …"). A newer database refuses an older binary with "unsupported database schema version N", so a downgrade needs the backup from before the update.
- Rooms are never deleted. When members leave, the room and its tasks stay, and anyone with its key can rejoin. There is no admin command to delete rooms or members.

### Run it as a service (systemd)

A user unit in `~/.config/systemd/user/schwitzer-server.service`, with the binary in `~/.local/state/schwitzer-server/bin` and the data in `~/.local/share/schwitzer-server`:

```ini
[Unit]
Description=schwitzer server

[Service]
Type=simple
ExecStart=%h/.local/state/schwitzer-server/bin/schwitzer-server --listen [::]:7444 --public-host example.org --data-dir %h/.local/share/schwitzer-server --auto-update
Restart=on-failure
RestartSec=2s

[Install]
WantedBy=default.target
```

```sh
systemctl --user daemon-reload
systemctl --user enable --now schwitzer-server
loginctl enable-linger "$USER"
journalctl --user -u schwitzer-server
```

Linger keeps the service running without a login and starts it at boot. systemd sends stdout to the journal, so the `server url:` line is there.

`--auto-update` replaces the binary and execs it under the same PID, so systemd sees no restart. Keep the binary in a folder the service user can write to; otherwise the server refuses to start with "cannot write beside <path>". [`deploy/schwitzer-server.service`](deploy/schwitzer-server.service) is the template the maintainers use.

### Updates and protocol versions

Every 2 minutes the server reads the release list and announces the newest stable release to the apps, whatever its own version and channel. Apps with self-update on the Release channel install it; apps on the Dev channel ignore it.

With `--auto-update` the server installs the same release. With `--update-channel dev` it follows GitHub's rolling `dev` prerelease instead: every third check (about every 6 minutes) it reads `releases/tags/dev` and installs the highest `schwitzer-server-<version>-linux-<arch>.tar.gz` there, even a lower version than its own when it came from the other channel. The default `--update-channel release` is what stkn.org runs. Either way:

- It downloads `schwitzer-server-<version>-linux-<arch>.tar.gz` (at most 64 MiB) and checks its size and sha256.
- It swaps the file, closes connections (apps reconnect at once) and execs the new binary.
- A failed install of a version is retried after 30 minutes.

Without `--auto-update`, update the server yourself soon after each release. When a release changes the protocol, updated apps refuse the older server ("the server speaks a different protocol version"), and older apps refuse a newer server, until both match. The release notes say when that happens.

`--releases-url` may point at any HTTP(S) endpoint that serves the same JSON as GitHub's release list (`tag_name`, `draft`, `prerelease`, `assets[]` with `name`, `browser_download_url`, `size`, `digest`). It only changes what the server announces and installs; apps still download from GitHub.

### Limits

|Limit|Refusal|
|---|---|
|50 rooms per member|"you are in 50 rooms already; leave one to create or join another"|
|requests: a burst of 60, then one per second per member|"too many requests, try again in N s"|
|room snapshot under 4 MiB|"the room is full: …"|
|task with its notes under 1 MiB|"the task is full: …"|

The app shows these refusals as "invalid request: …".

### Logs

`RUST_LOG` filters the log (default `info`); it goes to stderr, with no log file. At info the server logs "schwitzer server listening on …", "announcing release …", "installing server release …", "restarting into …" and "migrated the database …". Requests are not logged at info; `RUST_LOG=debug` shows more.

### Troubleshooting

|Symptom|Cause|Fix|
|---|---|---|
|the app shows "host:port does not answer"|UDP blocked, wrong `--public-host`, or the server is down|open the UDP port, fix `--public-host`, start the server|
|"the server speaks a different protocol version"|app and server versions differ|update the older side|
|"no such room"|the key is wrong, or belongs to another server or data dir|ask for the room key again|
|the server exits with "cannot write beside …"|`--auto-update` and the binary's folder is not writable|move the binary or make its folder writable|
|"no server release is built for this platform"|`--auto-update` outside Linux x86_64 and aarch64|drop `--auto-update` and update by hand|
|"unsupported database schema version N"|an older binary on a newer database|run the newer binary, or restore the backup from before the update|

## Build from source

See [CONTRIBUTING.md](CONTRIBUTING.md) for prerequisites, building, checks, commit rules and releases. [PRODUCT.md](PRODUCT.md) says what Schwitzer is for, [DESIGN.md](DESIGN.md) how it looks, and [AGENTS.md](AGENTS.md) is for coding agents working on this repo. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) shows how the pieces fit together and how agents work through the omp plugin.

## License

Schwitzer is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. The app lists the licenses of the open source components it is built from under Settings → About → Licenses.
