# Schwitzer — first cut

## Context
Source: CoffeeShop voice transcript 2026-10-17 22:56–23:14 (felix, tuco86). Schwitzer is a task manager for coding agents and their humans: "Agents fischen sich Tasks raus und schwitzen". Someone opens a room without signing up; the room has a key that is copied and shared. In a room, humans or agents post proposals. Humans mark them OK, agents turn them into plans, humans approve the plans, and agents work them off one by one, each claiming "the next free task". Prioritisation is light and there is explicitly no Kanban. Everyone sees who works on what, plus each member's live agent sessions: title, linked task, runtime and tokens burned. Each agent runs on its owner's machine and subscription, and nobody steers another person's agent.

First cut, "nur für uns":
- an iced desktop app (no wasm/web)
- `weida` as the net stack
- `iced_impeccable` built in
- a standalone `schwitzer-server` that hosts the rooms
- an MCP server inside the app for agents
- live sessions read from local omp session files

The repo is `/home/felix/code/schwitzer`: a git repo on `main` with no commits. It contains only `.omp/mcp.json`, which is local, holds a griasdi token and must stay untracked.

Reference implementations to copy patterns from (read them; don't depend on them):
- `/home/felix/code/griasdi` (iced + weida + iced_impeccable + rmcp)
- the weida source at the pinned rev: `~/.cargo/git/checkouts/weida-d61b50906505bda6/8e29fcb/`. Do not use `~/code/weida`; it is an old alpha.4 fork.
- `/home/felix/code/iced_impeccable`

## Approach

### 1. Workspace scaffold
- Root `Cargo.toml`:
  - Workspace: `resolver = "3"`, `members = ["crates/*"]`.
  - `[workspace.package]`: `version = "0.1.0"`, `edition = "2024"`, `rust-version = "1.91"`, `license = "LicenseRef-proprietary"`.
  - `[workspace.lints.rust] unsafe_code = "forbid"`.
  - Copy griasdi's dev profile verbatim: `[profile.dev] debug = "line-tables-only"` and `[profile.dev.package."*"] opt-level = 2, debug = false`.
- `[workspace.dependencies]`, using exactly these lines (taken from griasdi's `Cargo.toml`; weida is pinned to the exact rev because its wire protocol is alpha):
  ```toml
  weida = { git = "https://github.com/tuco86/weida.git", rev = "8e29fcb681fd36cd4ae668c991391a0f3d098519" }
  iced = { version = "0.14", features = ["advanced", "tokio", "markdown"] }
  iced_impeccable = { git = "https://github.com/tuco86/iced_impeccable.git", rev = "4c90fe0cfc87c90fa43693d1d299db35e0a1021c" }
  tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "fs", "io-util", "net"] }
  futures = "0.3"
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  postcard = { version = "1.1", features = ["use-std"] }
  thiserror = "2"
  anyhow = "1"
  tracing = "0.1"
  tracing-subscriber = { version = "0.3", features = ["env-filter"] }
  rand = "0.8"
  hex = "0.4"
  blake3 = "1.8"
  rusqlite = { version = "0.37", features = ["bundled"] }
  toml = "1"
  clap = { version = "4", features = ["derive"] }
  directories = "6"
  tempfile = "3"
  rmcp = { version = "3.5", default-features = false, features = ["server", "macros", "transport-streamable-http-server"] }
  axum = { version = "0.8", default-features = false, features = ["http1", "tokio"] }
  tokio-util = "0.7"
  ```
  - weida's `swarm` feature and `weida-raft` are not used.
  - If weida fails to build without `swarm`, add `features = ["swarm"]` and change nothing else.
- Four crates, each with `version/edition/rust-version.workspace = true` and `[lints] workspace = true`:
  - `crates/schwitzer-proto`: library, wire types.
  - `crates/schwitzer-server`: library plus a bin named `schwitzer-server`.
  - `crates/schwitzer-client`: library, the network client.
  - `crates/schwitzer-app`: bin named `schwitzer`, the iced app. It has `[features] remote = ["dep:iced_impeccable"]` and `iced_impeccable = { workspace = true, optional = true }`.
- `.gitignore`:
  - `/target`, `/.omp/mcp.json`, `/.omp/skills/impeccable/scripts/bin/`, `/.omp/plugins/node_modules/`
  - the `# impeccable-ignore-start` block, copied verbatim from `~/code/griasdi/.gitignore` (starts at line 15, ends at its matching end marker).
- impeccable tooling for agents in this repo:
  - Copy `~/code/griasdi/.omp/skills/impeccable/` to `.omp/skills/impeccable/` without `scripts/bin/`.
  - In the repo root, run `omp plugin install iced-impeccable@iced-impeccable --scope=project`.
  - If omp reports an unknown marketplace, first run `omp plugin marketplace add /home/felix/code/iced_impeccable`, then retry.
  - This should produce `.omp/plugins/installed_plugins.json`, as in griasdi.

### 2. `schwitzer-proto`: wire types
Dependencies: serde, postcard, thiserror, hex, rand, blake3, tokio (`io-util`, for the frame reader only). Postcard is positional, so enum variants are append-only.

**Modules copied from griasdi:**
- `frame.rs`: copy `~/code/griasdi/crates/griasdi-proto/src/frame.rs` with `encode_frame`, `read_frame` and `FrameError`.
- `lib.rs`: copy the `to_bytes`/`from_bytes`/`now_ms` helpers from griasdi-proto `lib.rs`.
- `ids.rs`: copy the `byte_id!`/`random_id!` macros from griasdi-proto `src/ids.rs`. Define:
  - `MemberId([u8;32])`: the sha256 SPKI fingerprint of the app's weida key, `fp.as_bytes()`.
  - `RoomToken([u8;32])`: random; the room secret.
  - `RoomId([u8;32])`: `blake3(token)`, via `RoomId::of(&RoomToken)`.
  - `ClaimToken([u8;16])`: random.

**Constants:**
- `PROTOCOL_VERSION: u32 = 1`
- `PATH_RPC = "/rpc"`, `PATH_EVENTS = "/events"`
- `MAX_RPC_BYTES = 1 << 20`, `MAX_EVENT_FRAME = 4 << 20`, `RPC_TIMEOUT = 15 s`
- Field caps, all in bytes:

| Field | Cap |
|---|---|
| room name | 100 |
| title | 200 |
| body | 64 KiB |
| plan | 256 KiB |
| note | 16 KiB |
| agent label | 100 |
| member name | 64 |
| sessions per publish | 50 |
| session title | 200 |

**`addr.rs`:**
- `ServerAddr { host: String, port: u16, fingerprint: [u8;32] }`
  - `url(path) -> "weida://sha256:<hex>@host:port<path>"`, with IPv6 hosts bracketed; copy griasdi `ServerAddr::url`.
  - `Display` = `url("")`.
  - `FromStr` accepts exactly that form with an optional trailing `/`.
- `RoomKey { server: ServerAddr, token: RoomToken }`
  - `Display`: `schwitzer://sha256:<64hex>@<host>:<port>/<64hex token>`.
  - `FromStr` is the inverse. It trims whitespace and returns a parse error naming the bad part: scheme, fingerprint, host, port or token.

**`model.rs`** (all derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`):
```rust
pub enum Stage { Proposed, Accepted, Planned, Ready, Done, Rejected }
pub enum Priority { High, Normal, Low }            // sort order: High first
pub enum Work { Plan, Build }
pub enum Decision { Accept, Reject, Approve, Replan, Reopen }
pub struct Claim { pub member: MemberId, pub agent: String, pub work: Work, pub since_ms: u64, pub token: ClaimToken }
pub struct Note { pub author: MemberId, pub agent: Option<String>, pub text: String, pub at_ms: u64 }
pub struct Task { pub number: u64, pub title: String, pub body: String, pub plan: String, pub stage: Stage,
                  pub priority: Priority, pub author: MemberId, pub created_ms: u64, pub updated_ms: u64,
                  pub claim: Option<Claim>, pub notes: Vec<Note> }
pub struct Member { pub id: MemberId, pub name: String, pub online: bool }
pub struct SessionInfo { pub id: String, pub title: String, pub project: String, pub model: String,
                         pub started_ms: u64, pub active_ms: u64, pub tokens: u64, pub task: Option<u64> }
pub struct RoomState { pub id: RoomId, pub name: String, pub members: Vec<Member>, pub tasks: Vec<Task>,
                       pub sessions: Vec<(MemberId, Vec<SessionInfo>)> }
```
- "In progress" is not a stage. It is `Ready` with `claim: Some(work: Build)`.
- "Being planned" is `Accepted` with a Plan claim.

**`rpc.rs`:**
```rust
pub enum Request {
  Hello { protocol: u32, name: String },
  SetName { name: String },
  CreateRoom { name: String },
  JoinRoom { token: RoomToken },
  LeaveRoom { room: RoomId },
  Propose { room: RoomId, title: String, body: String },
  EditTask { room: RoomId, task: u64, title: String, body: String, plan: String },
  SetPriority { room: RoomId, task: u64, priority: Priority },
  Decide { room: RoomId, task: u64, decision: Decision },
  Claim { room: RoomId, work: Work, task: Option<u64>, agent: String },
  SubmitPlan { room: RoomId, task: u64, plan: String },
  Finish { room: RoomId, task: u64, note: String },
  Release { room: RoomId, task: u64, note: String },
  AddNote { room: RoomId, task: u64, text: String, agent: Option<String> },
  PublishSessions { room: RoomId, sessions: Vec<SessionInfo> },
}
pub enum Response { Hello { member: MemberId }, Ok, Room { id: RoomId, token: RoomToken }, Task(Task), Claimed(Option<Task>) }
#[derive(thiserror::Error, ...)]
pub enum ApiError { Protocol, NotAuthenticated, NotMember, NoSuchRoom, NoSuchTask, WrongStage(Stage), Claimed,
                    NotYourClaim, Invalid(String), Internal }
pub type Reply = Result<Response, ApiError>;
```
- The `#[error]` messages are human-readable sentences, e.g. `WrongStage` → "task is in stage {0:?}". The UI and MCP show them verbatim.

**`events.rs`:**
- `Subscribe {}` (empty struct). The server knows the member's rooms.
- Events:
```rust
pub enum Event { Room(RoomState), Ready, Task { room: RoomId, task: Task }, Member { room: RoomId, member: Member },
                 Sessions { room: RoomId, member: MemberId, sessions: Vec<SessionInfo> }, Left { room: RoomId } }
```

**Unit tests in proto:**
- `RoomKey`/`ServerAddr` round-trip: hostname, IPv4, IPv6 `[::1]`.
- Each malformed part of a key (scheme, fingerprint, host, port, token) gives its own error.

### 3. `schwitzer-server`
Dependencies: proto, weida, tokio, rusqlite, clap, anyhow, tracing, tracing-subscriber, rand, blake3.

**`lib.rs`:**
- `pub struct Options { pub listen: SocketAddr, pub public_host: String, pub data_dir: PathBuf }`
- `pub async fn start(opts: Options) -> anyhow::Result<Server>`
- `pub struct Server { pub addr: ServerAddr, tasks: JoinSet<&'static str> }` with `pub async fn ended(&mut self) -> &'static str`

**Startup in `start`:**
- Identity: `IdentitySource::files(data_dir.join("identity"), FilesOptions { names: vec![public_host.clone()], ..FilesOptions::default() })`. The key persists across restarts, so room keys stay valid.
- Runtime: `Runtime::new(RuntimeConfig::default())`.
- Bind: `listener.bind_quic(listen, ServerTls::new(source).require_client(ClientTrust::AnyKey))`.
- Endpoints: `listener.replier(PATH_RPC)`, `listener.replier(PATH_EVENTS)`.
- Spawn the two serve loops into the `JoinSet` with the griasdi `spawn!` pattern from `griasdi-server/src/lib.rs` (around lines 335–410).
- `addr`: `ServerAddr { host: public_host, port: binding.local_addr().port(), fingerprint }`.

**`main.rs`:**
- clap flags: `--listen` (default `0.0.0.0:7444`), `--public-host` (default `127.0.0.1`), `--data-dir` (default `./schwitzer-data`).
- tracing `EnvFilter` default `info`.
- Prints exactly one stdout line: `server url: {addr}`. That is the string users paste into "Create room".
- Exits non-zero when any endpoint task ends.

**Auth (`auth.rs`):**
- Caller = `request.meta().peer`, which must be `Some(PeerIdentity::Key(fp))`, giving `MemberId(*fp.as_bytes())`. Otherwise reply `ApiError::NotAuthenticated`.
- Every room-scoped request checks membership in the store and returns `NotMember` / `NoSuchRoom`.

**`store.rs`: SQLite**
- Copy griasdi's `Store` from `griasdi-server/src/store/mod.rs` lines 374–430: WAL, `synchronous=NORMAL`, `busy_timeout` 5 s, one connection behind a mutex, `async fn run(f)` on `spawn_blocking`.
- Each mutation runs in one transaction. This makes `Claim` selection plus update atomic.
- Schema at `user_version = 1`:
  ```sql
  CREATE TABLE people(member BLOB PRIMARY KEY, name TEXT NOT NULL);
  CREATE TABLE rooms(id BLOB PRIMARY KEY, name TEXT NOT NULL, created_ms INTEGER NOT NULL, next_number INTEGER NOT NULL DEFAULT 1);
  CREATE TABLE members(room BLOB NOT NULL, member BLOB NOT NULL, joined_ms INTEGER NOT NULL, PRIMARY KEY(room, member));
  CREATE TABLE tasks(room BLOB NOT NULL, number INTEGER NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL, plan TEXT NOT NULL,
    stage INTEGER NOT NULL, priority INTEGER NOT NULL, author BLOB NOT NULL, created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL,
    claim_member BLOB, claim_agent TEXT, claim_work INTEGER, claim_since_ms INTEGER, claim_token BLOB, PRIMARY KEY(room, number));
  CREATE TABLE notes(room BLOB NOT NULL, number INTEGER NOT NULL, at_ms INTEGER NOT NULL, author BLOB NOT NULL, agent TEXT, text TEXT NOT NULL);
  ```
- Stage, priority and work are stored as explicit integer mappings in `store.rs`, listed in declaration order starting at 0. They are never postcard bytes.
- On open:
  - `user_version` 0 → create the schema.
  - `user_version` 1 → use as is.
  - Anything else → refuse to start, with an error naming the version.

**`rpc.rs`:**
- Accept loop as in griasdi `rpc/mod.rs`: `accept()`, spawn per request, `body().read_capped(MAX_RPC_BYTES)`, `from_bytes::<Request>`, dispatch, `reply(TransferMeta::default())`, `write_all(&to_bytes(&reply))`, `finish()`.
- Validation: every text field is trimmed and capped. Title, room name and member name must be non-empty. Violations → `Invalid("<field> …")`.

**Dispatch rules:**
- `Hello`
  - A protocol mismatch → `Protocol`.
  - Upserts `people.name`.
  - Returns `Hello { member }`.
- `SetName`: upserts the name, then publishes `Member` to every room of the caller.
- `CreateRoom`
  - Generates the token with `rand`.
  - Inserts the room and adds the caller as a member.
  - Returns `Room { id, token }`.
  - Pushes `Event::Room(state)` to the caller.
- `JoinRoom`
  - Hashes the token and looks it up; an unknown token → `NoSuchRoom`.
  - Inserts the membership, idempotently.
  - On a new membership, pushes `Room(state)` to the joiner and `Member` to the others.
  - Returns `Room { id, token }`.
- `LeaveRoom`
  - Deletes the membership.
  - Pushes `Left` to the leaver and `Member` (`online:false`) to the others.
  - The room and its tasks stay.
- `Propose`: `number = next_number`, increments it, stage `Proposed`, priority `Normal`, empty plan.
- `EditTask`: allowed unless the stage is `Done` or `Rejected`.
- `SetPriority` and `AddNote`: allowed in any stage.
- `Decide`:

| Decision | From stage | To stage | Extra |
|---|---|---|---|
| Accept | Proposed | Accepted | |
| Reject | Proposed, Accepted, Planned | Rejected | clears the claim |
| Approve | Planned | Ready | needs a non-empty plan, else `Invalid("plan is empty")` |
| Replan | Planned, or Ready without a claim | Accepted | keeps the plan text as a draft |
| Reopen | Rejected, Done | Proposed | |

  - Any other combination → `WrongStage(current)`.
- `Claim`
  - `work: Plan` targets `Accepted`; `Build` targets `Ready`; only unclaimed tasks qualify.
  - `task: None` picks the first by `(priority High→Low, number asc)` and returns `Claimed(None)` when none is free.
  - `task: Some(n)`:
    - wrong stage → `WrongStage`
    - already claimed → `Claimed`
  - Sets the claim with a fresh `ClaimToken`, the caller and `since_ms = now`.
  - Returns `Claimed(Some(task))`.
- `SubmitPlan`
  - Allowed when the stage is `Accepted` with a Plan claim by the caller (else `NotYourClaim`), or when the stage is `Planned` (refinement, anyone).
  - Sets the plan, stage `Planned` and clears the claim.
- `Finish`
  - Needs stage `Ready` with a Build claim by the caller (else `NotYourClaim`).
  - Sets stage `Done`, clears the claim, appends `note` when non-empty, with `agent` = the claim's agent.
- `Release`
  - Needs an existing claim (else `Invalid("not claimed")`). Any member may release, so humans can free stuck claims.
  - Clears the claim and appends `note` when non-empty.
- `PublishSessions`
  - Replaces the caller's session list for that room. The list is held in memory only (`Mutex<HashMap<(RoomId, MemberId), Vec<SessionInfo>>>`).
  - Publishes `Sessions` to the room.
- After every task mutation, the server re-reads the full `Task` (notes included) inside the transaction. After commit, it publishes `Event::Task` to all room members (the griasdi Outbox-after-commit rule).

**`hub.rs`** (simplified copy of griasdi `hub.rs`):
- `HashMap<MemberId, Vec<(u64 /*sub id*/, mpsc::Sender<Arc<[u8]>>)>>` with queue size 1024.
- `publish(members, &Event)` encodes the frame once with `encode_frame` and uses `try_send`.
- A full queue removes that subscription. Its stream then ends and the client reconnects and resnapshots.

**`events.rs`** (pattern: griasdi `griasdi-server/src/events.rs`):
1. Authenticate the caller.
2. Read `Subscribe`.
3. Take `request.canceled()` before `reply`.
4. Register with the hub before the backlog.
5. `reply`, then write one `Room(state)` frame per room of the member, then `Ready`.
6. Loop with `select!` over the hub receiver and `canceled`.

- Presence:
  - When the member's first stream opens → publish `Member { online: true }` to their rooms.
  - When the last stream closes → `Member { online: false }`, drop their in-memory sessions and publish empty `Sessions` per room.
- `RoomState.members[].online` is computed from the hub.

### 4. `schwitzer-client`
Dependencies: proto, weida, tokio, futures, thiserror, tracing, rand.

**Construction and runtime:**
- `pub struct Client(Arc<Inner>)` is `Clone`.
- `pub fn start(identity_dir: &Path, name: String) -> Result<Client, ClientError>` builds:
  - its own tokio runtime (`new_multi_thread().worker_threads(2).thread_name("schwitzer-net").enable_all()`);
  - `weida::Runtime::with_handle(handle, RuntimeConfig { reconnect: ReconnectPolicy { max: 5s, ..Default::default() }, ..Default::default() })`;
  - the identity: `IdentitySource::files(identity_dir, FilesOptions::default())`;
  - `MemberId` from `source.fingerprint()`.
  - Copy the runtime and TLS bits from `griasdi-client/src/lib.rs` (604–646) and `conn.rs`: `ClientTls::new(Trust::by_address()).with_identity(source.clone())`.

**Public API:**
```rust
pub fn member(&self) -> MemberId
pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent>        // capacity 1024
pub fn rooms(&self) -> Vec<(RoomKey, Option<RoomState>)>             // snapshot for resync
pub fn add_room(&self, key: RoomKey)                                 // remember key, ensure server loop
pub async fn create_room(&self, server: ServerAddr, name: String) -> Result<RoomKey, ClientError>
pub async fn join_room(&self, key: RoomKey) -> Result<RoomId, ClientError>
pub async fn leave_room(&self, room: RoomId) -> Result<(), ClientError>   // forgets the key too
pub async fn request(&self, room: RoomId, req: Request) -> Result<Response, ClientError> // routes to the room's server
pub async fn set_name(&self, name: String)                          // SetName on every online server
pub fn shutdown(&self)
#[derive(Debug, Clone)] pub enum ClientEvent { Status { server: ServerAddr, online: bool, error: Option<String> },
                                              Room(Box<RoomState>), RoomRemoved(RoomId) }
pub enum ClientError { Offline, Timeout, Api(ApiError), Net(String), Proto(String) }
```
- `ClientError` implements `Display` via thiserror.

**Connection management:**
- One `ServerConn` per distinct `ServerAddr`, kept in a `Mutex<HashMap<ServerAddr, Arc<ServerConn>>>`. It holds `watch::Sender<Option<Arc<Session>>>`.
- A server loop (pattern: `griasdi-client/src/sync.rs` `server_loop` and `run_session`, plus `backoff.rs` 1 s → 30 s) does, per session:
  1. Create one `Requester` per path and connect `addr.url(PATH_RPC)` / `addr.url(PATH_EVENTS)` with a 10 s timeout.
  2. Send `Hello`.
  3. Send `JoinRoom { token }` for every remembered key of that server:
     - `NoSuchRoom` → emit `Status { error: Some("room <name or id8> no longer exists") }` and keep going.
  4. Publish the session in the watch.
  5. Open `/events`: `open`, write `Subscribe`, `finish`, `reply.recv()`, then the `read_frame` loop.
- Event handling:
  - `Room` / `Task` / `Member` / `Sessions` update `Mutex<HashMap<RoomId, RoomState>>` and emit `Room(clone)`.
  - `Left` removes the room and emits `RoomRemoved`.
  - `Ready` resets the backoff.
- Session end → emit `Status { online: false, error }`, then sleep for the backoff.
- `request` / `create_room`:
  - Wait up to 10 s on the watch for a session (else `Offline`).
  - Exchange with `RPC_TIMEOUT` exactly as `griasdi-client` `conn.rs` does.
  - `create_room` builds the `RoomKey` from `server` + the returned token, then calls `add_room`.

**Integration test `crates/schwitzer-client/tests/flow.rs`** (dev-dep `schwitzer-server`, `tempfile`; `#[tokio::test(flavor = "multi_thread")]`). Start the server in-process on `127.0.0.1:0` with a temp data dir, and two clients A and B, each with its own temp identity dir. Assert:
1. A `create_room` → B `join_room(key)` → B's `ClientEvent::Room` contains A as a member.
2. Proposals at `Normal` and `High` priority.
3. `Claim { work: Build }` on a `Proposed` task → `WrongStage(Proposed)`.
4. Accept both → B `Claim { work: Plan, task: None }` returns the High one first.
5. A `SubmitPlan` on B's plan claim → `NotYourClaim`.
6. B `SubmitPlan` → `Planned`; A Approve → `Ready`.
7. B claims Build → B `Finish` with a note → `Done` with the note; A receives a `ClientEvent::Room` reflecting `Done`.
8. Restart the server on the same data dir (drop it, then `start` again on the port it had bound): the room key still joins and the tasks are still present.

### 5. `schwitzer-app`: iced UI
Dependencies: proto, client, iced, iced_impeccable (optional), tokio, futures, serde, toml, directories, rmcp, axum, tokio-util, serde_json, blake3, rand, tracing, tracing-subscriber.

**`settings.rs`:**
- Data dir = `$SCHWITZER_HOME`, else `ProjectDirs::from("", "", "schwitzer").data_dir()`.
- `settings.toml`:
  ```toml
  name = "felix"                  # default: $USER / $USERNAME, else "anon"
  mcp_port = 13339
  omp_sessions_dir = "/home/felix/.omp/agent/sessions"   # default ~/.omp/agent/sessions
  last_server = "weida://sha256:…@host:7444"              # optional, prefills Create room
  rooms = ["schwitzer://sha256:…@host:7444/<token>"]
  ```
- Missing file → defaults. Unparsable file → start with defaults and show the parse error in the UI banner. Never overwrite the bad file until the user changes a setting.
- Save = write `settings.toml.tmp`, then rename.
- `identity/` lives in the data dir and is passed to `Client::start`.

**`main.rs`:**
- `iced::application(App::boot, App::update, App::view).title(..).subscription(App::subscription)`.
- Wrap it exactly like griasdi `griasdi-desktop/src/main.rs` (around lines 95–135), but with an `application` instead of a `daemon`:
  - `#[cfg(feature = "remote")] iced_impeccable::Remote::new(app).heartbeat(|m| matches!(m, Message::Tick | Message::LocalSessions(_))).run()`
  - `#[cfg(not(feature = "remote"))] app.run()`.

**`bridge.rs`:**
- Copy griasdi `griasdi-desktop/src/bridge.rs` lines 12–47: `ClientKey` with a constant hash, `client_events` with `iced::stream::channel`, and `Lagged` → `Message::Resync`.
- `Resync` reloads `client.rooms()`.
- Copy the `call`/`fire` helpers from griasdi `app/mod.rs` (1430–1455).

**State and messages:**
- State:
  - `rooms: Vec<(RoomKey, Option<RoomState>)>` in settings order
  - `selected: Option<RoomId>`, `open_task: Option<u64>`
  - per-server status
  - form fields
  - `banner: Option<String>` for the last error, dismissable
  - `local_sessions`
  - `mcp` status
- Messages include `Tick` (`iced::time::every(1s)`, for runtime displays), `Client(ClientEvent)`, `Resync`, `LocalSessions(Vec<LocalSession>)` and one message per user action below.

**Layout:**
- **Left sidebar.**
  - Rooms list: name plus a server online dot.
  - Buttons `#create-room` and `#join-room`, which open inline forms:
    - Create: `#server-url` (prefilled `last_server`), `#room-name`, `#create-submit`.
    - Join: `#room-key`, `#join-submit`.
  - Footer:
    - `#name-input` (submit → `set_name` + save)
    - MCP status text (`MCP: http://127.0.0.1:<port>/mcp` or the bind error)
    - `#copy-mcp-config`, which copies `{"mcpServers":{"schwitzer":{"type":"http","url":…,"headers":{"Authorization":"Bearer <token>"}}}}` (shape from griasdi `mcp/serve.rs` `client_config`).
- **Room header.**
  - Room name.
  - `#copy-key`, which copies the `RoomKey` string with `iced::clipboard::write`.
  - `#leave-room`.
  - Member chips: name plus an online dot.
- **Sessions strip.**
  - One row per published session of any member: member name, title, project, `#n` link (opens the task), runtime `now - started_ms` (h:mm), tokens (`12.3k`), and "idle" when `active_ms` is older than 2 minutes.
  - Empty → "No agent sessions on tasks".
- **Task list.** One scrollable `#tasks`, grouped in this order:
  - **Proposals** (Proposed), headed by the new-proposal form `#proposal-title`, `#proposal-body` (text_editor) and `#propose`.
  - **Planning** (Accepted and Planned).
  - **Ready** (Ready, unclaimed).
  - **In progress** (Ready, claimed).
  - **Done / Rejected**: collapsed toggle `#show-closed`.
  - Within a group, rows sort by priority, then number.
  - Row: `#n`, title, priority badge, and claim text `"<member> · <agent> · <work> · <since>"`.
  - Each row is a button with id `task-<n>`.
- **Detail pane** (right; open when `open_task` is set):
  - Title and body, editable via `#edit-task` → text inputs → `#save-task` (`EditTask`).
  - Plan rendered with `iced::widget::markdown`; `#edit-plan` toggles a text_editor and `#save-plan` (`EditTask`).
  - Priority buttons `#prio-high`, `#prio-normal`, `#prio-low`.
  - Stage action buttons, shown only when legal per the step-3 table: `#accept`, `#reject`, `#approve`, `#replan`, `#reopen`; `#release` when claimed.
  - Notes list (author name or agent, time, text).
  - `#note-input` + `#add-note`.
- Every RPC failure shows `ClientError`'s Display text in the banner.
- Styling uses iced built-in themes (`Theme::Dark`). Design polish is left to a later impeccable pass.

### 6. `sessions.rs`: omp session watcher (in schwitzer-app)
- `Subscription::run(watch)` uses `iced::stream::channel` and a `tokio::time::interval(5s)`. It emits `Message::LocalSessions(Vec<LocalSession>)` only when the list changed.
- `LocalSession { id, title, project, model, started_ms, active_ms, tokens, markers: Vec<ClaimToken> }`.

**Scan:**
- Top-level session files: `<omp_sessions_dir>/*/*.jsonl`. Each one's subagent files are `<same path without .jsonl>/*.jsonl`.
- A session is active when the max mtime over the top-level file and its subagent files is within the last 10 minutes; `active_ms` = that mtime.
- Only active sessions are parsed.

**Parsing:**
- Incremental per file: keep the byte offset and any partial trailing line. Reset to 0 if the file shrank.
- Each line is parsed as `serde_json::Value`; lines that fail are skipped.

| Line | Gives |
|---|---|
| `type == "session"` | `id`, `timestamp` → `started_ms`, `cwd` (last path component → `project`), `title` |
| `type == "title"` or `"title_change"` | `title`, latest wins |
| `type == "model_change"` | `model` |
| `type == "message"` with `message.role == "assistant"` | `tokens += message.usage.totalTokens` |
| `type == "message"` with `message.role == "toolResult"` | scan every `content[].text` for `schwitzer-claim: <32 hex>` |

- Subagent files contribute tokens only.
- Markers are kept in order of appearance.

**Linking and publishing (in `App::update`):**
- A session's task = the latest marker that equals the `claim.token` of a currently claimed task in a joined room.
- For each room, publish via `Request::PublishSessions` the sessions linked to that room's tasks; set `SessionInfo.task = number`.
- Publish only when a room's list differs from what was last sent. An empty list is sent once when a session drops out.
- Unlinked sessions are never published.
- Missing sessions dir → empty list, no error.

**Unit tests** on a fixture jsonl in `crates/schwitzer-app/tests/fixtures/`, built from the line shapes in the table above:
- the token sum, including a subagent file
- the latest title
- marker extraction
- incremental append (appending lines updates the totals)

### 7. MCP server (in schwitzer-app, `src/mcp/`)
- Copy `~/code/griasdi/crates/griasdi-desktop/src/mcp/serve.rs` and `token.rs`:
  - rmcp `StreamableHttpService` on axum.
  - Bearer guard: blake3 constant-time compare; reject requests that carry `Origin`.
  - Bind `127.0.0.1:<mcp_port>`.
  - Token: `<data dir>/mcp-token`, 64 hex, mode 0600.
  - Started as an iced `Subscription::run_with(McpKey { client, port, token }, serve)`.
  - Status flows back as `Message::Mcp(McpStatus)`.
  - Drop griasdi's update-restart bind deadline. Keep the 10 s bind retry after `AddrInUse`, and show "port in use" in the footer.
- MCP is always on and needs no setting.

**Tools** (`tools.rs`, `#[tool_router]` pattern from griasdi `tools.rs`):
- Args derive `Deserialize, schemars::JsonSchema`. Results are `serde_json::to_string_pretty` text. Each call has a 30 s timeout.
- Errors are `CallToolResult::error` with `ClientError`'s Display text.
- `room` arg: a room name (case-insensitive exact match) or a room id hex prefix of ≥4 characters.
  - If omitted and exactly one room is joined, use it.
  - Otherwise error with the list of rooms.

| Tool | Args | Result |
|---|---|---|
| `list_rooms` | — | `[{room: id8, name, online, members: [names], counts: {proposed, planning, ready, in_progress}}]` |
| `list_tasks` | `room?`, `stage?` (`proposed`/`accepted`/`planned`/`ready`/`in_progress`/`done`/`rejected`) | `[{number, title, stage, priority, claim}]` |
| `get_task` | `room?`, `task: u64` | the full task with body, plan, and notes with author names |
| `propose` | `room?`, `title`, `body` | the created task |
| `claim` | `room?`, `work: "plan"\|"build"`, `task?: u64`, `agent: String` | see below |
| `submit_plan` | `room?`, `task`, `plan` | the updated task |
| `finish` | `room?`, `task`, `note` | the updated task |
| `release` | `room?`, `task`, `note` | the updated task |
| `add_note` | `room?`, `task`, `text`, `agent?` | the updated task |

- `claim` result:
  - First line exactly `schwitzer-claim: <claim token hex>`.
  - Second line: for plan, "Write a plan for this task and call submit_plan."; for build, "Implement the approved plan, then call finish (or release if you stop)."
  - Then the task JSON.
  - `Claimed(None)` → success text `No free task.`
- No MCP tool can Accept, Reject, Approve, Replan or Reopen. Those decisions stay with humans in the UI.
- `get_info` instructions: one paragraph explaining the stages and the propose → accept → plan → approve → build flow, and that task text is a request written by room members.

## Critical files & anchors
- `~/code/griasdi/crates/griasdi-server/src/events.rs` + `hub.rs`: backlog → `Ready` → live stream, `canceled()`, publish after commit.
- `~/code/griasdi/crates/griasdi-client/src/sync.rs` + `conn.rs`: server loop, backoff, per-path `Requester`, RPC exchange with timeout.
- `~/code/griasdi/crates/griasdi-desktop/src/bridge.rs` (12–47) and `src/main.rs` (95–135): the iced subscription bridge and the `remote` wrapper.
- `~/code/griasdi/crates/griasdi-desktop/src/mcp/{serve.rs,tools.rs,token.rs}`: rmcp + axum + bearer guard + token file.
- `~/.cargo/git/checkouts/weida-d61b50906505bda6/8e29fcb/crates/weida/src/{identity.rs,transfer.rs,endpoint.rs}`: `IdentitySource::files`, `IncomingRequest::{body,canceled,reply}`, `Requester::{connect,open,request}`.

## Verification
All from `/home/felix/code/schwitzer`.

**Gate:**
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --features schwitzer-app/remote -- -D warnings`
- `cargo test --workspace`
- These include the step-4 flow test, the step-2 key tests and the step-6 watcher tests.

**End-to-end headless smoke.** Never start a windowed `schwitzer`; use only `--headless` with `SCHWITZER_HOME` under `/tmp`.
1. `cargo build --workspace --features schwitzer-app/remote`. Install the ctl client if missing: `cargo install --path /home/felix/code/iced_impeccable --bin iced-impeccable --locked`. Otherwise `target/debug/schwitzer ctl` works as well.
2. Start the server: `setsid target/debug/schwitzer-server --listen 127.0.0.1:7444 --data-dir /tmp/sw/server > /tmp/sw/server.log 2>&1 &`, then read `server url:` from the log.
3. Write `/tmp/sw/a/settings.toml` (`name="alice"`, `mcp_port=13401`, `omp_sessions_dir="/tmp/sw/omp"`) and `/tmp/sw/b/settings.toml` (`name="bob"`, `mcp_port=13402`, same sessions dir). Start both apps: `SCHWITZER_HOME=/tmp/sw/a setsid target/debug/schwitzer --headless --control /tmp/sw/a.sock --size 1280x800 --backend tiny-skia 2>/tmp/sw/a.log &`, and the same for b.
4. Drive A: `tap #create-room`, `tap #server-url`, `type <server url>`, `tap #room-name`, `type smoke`, `tap #create-submit`, `wait-for smoke`, `tap #copy-key`, `clip`. The clip must start with `schwitzer://sha256:`.
5. Drive B: `tap #join-room`, `tap #room-key`, `type <key>`, `tap #join-submit`, `wait-for smoke`, `wait-for alice`.
6. A proposes "Smoke task" (`#proposal-title`, `#propose`). On B: `wait-for "Smoke task"`, `tap ~Smoke task`, `tap #accept`. On A: `tap ~Smoke task`, `wait-gone #accept`, `wait-for #reject` (the stage is now Accepted).
7. MCP against A: read `/tmp/sw/a/mcp-token` and POST to `http://127.0.0.1:13401/mcp`.
   - Headers: `Authorization: Bearer <token>`, `Content-Type: application/json`, `Accept: application/json, text/event-stream`.
   - Send `initialize` (`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}`) and take the `mcp-session-id` response header.
   - Then `notifications/initialized`, then `tools/call` `claim` with `{"work":"plan","agent":"smoke"}`. The result starts with `schwitzer-claim: `.
   - Without the bearer → HTTP 401.
8. Session link: write `/tmp/sw/omp/-tmp-proj/s1.jsonl` containing:
   - a `session` line (`cwd:"/tmp/proj"`, title "Smoke session")
   - an assistant message with `usage.totalTokens: 1234`
   - a `toolResult` message whose text contains the claim line from step 7
   Within 10 s, B shows "Smoke session" with `#1` and `1.2k` (`wait-for "Smoke session"`).
9. MCP `submit_plan` (task 1, plan "Do it") → B `tap #approve` → MCP `claim` build → `finish` with note "smoke finished". On A: `tap #show-closed`, `tap ~Smoke task`, `wait-for "smoke finished"`, `wait-for #reopen` (the stage is now Done).
10. Take `screenshot --annotate /tmp/sw/a.png` and look at it, then `quit` both apps and kill the server. Server restart: start the server again on the same data dir, start A again, and `wait-for smoke`: the room survived.

## Assumptions & contingencies
- **Session visibility.** Only omp sessions that hold a claim marker for a task in the room are published to that room. Unrelated local sessions never leave the machine. The user can widen this later, e.g. with a per-room "share all sessions" toggle.
- **Agents never decide.** MCP has no Accept/Reject/Approve/Replan/Reopen. Humans decide in the UI, as the transcript describes ("von uns als okay markiert").
- **Deployment and git.** No server deployment and no commit or push in this cut. The server runs locally for the smoke, and the repo stays uncommitted for review.
- **Contingency: rmcp.** If rmcp 3.5's macro or session API differs from griasdi's code, match the version griasdi's `Cargo.lock` resolves (3.5.1).
- **Contingency: iced 0.14 API.** If a 0.14 API differs from what griasdi uses, follow griasdi's usage, since it compiles against 0.14.0.
