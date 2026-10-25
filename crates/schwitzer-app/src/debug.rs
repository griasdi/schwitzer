//! Debug sessions on this machine: the `out` folder whose files go to the other participants,
//! the `in` folder their files are written into, and the worker that moves the bytes.
//!
//! Layout under the data directory, per session:
//! `debug/<session>/out/<stream>` (written by the member, streamed) and
//! `debug/<session>/in/<member dir>/<stream>` (received).

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use futures::channel::mpsc::Sender;
use futures::{SinkExt, Stream};
use iced::Subscription;
use schwitzer_client::{Client, ClientError, ClientEvent};
use schwitzer_proto::{ApiError, DebugId, Member, MemberId, RoomId, caps, valid_stream_name};
use tokio::sync::broadcast::error::RecvError;

/// How often the out folder is looked at.
const SEND_EVERY: Duration = Duration::from_millis(500);
/// A received file stops growing at this size; further data is dropped.
const MAX_IN_FILE: u64 = 256 * 1024 * 1024;
/// Session folders untouched this long are removed when a worker starts.
const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Bytes of a member folder's name part.
const MEMBER_DIR_NAME: usize = 32;

/// The folder of all debug sessions under the data directory.
const SESSIONS_DIR: &str = "debug";
/// The folder of a session that holds the files this member sends.
const OUT_DIR: &str = "out";
/// The folder of a session that holds the files received from the others.
const IN_DIR: &str = "in";

/// Where one session keeps its files on this machine.
pub struct SessionDirs {
    sessions: PathBuf,
    short: String,
}

impl SessionDirs {
    pub fn new(data_dir: &Path, session: DebugId) -> Self {
        Self {
            sessions: data_dir.join(SESSIONS_DIR),
            short: session.short(),
        }
    }

    /// The folder that holds the folders of all sessions.
    pub fn sessions(&self) -> &Path {
        &self.sessions
    }

    /// The folder whose files are sent.
    pub fn out(&self) -> PathBuf {
        self.sessions.join(&self.short).join(OUT_DIR)
    }

    /// The folder that receives the others' files, one folder per member.
    pub fn inbox(&self) -> PathBuf {
        self.sessions.join(&self.short).join(IN_DIR)
    }

    /// [`Self::out`] relative to the data directory, with `/` separators.
    pub fn out_rel(&self) -> String {
        format!("{SESSIONS_DIR}/{}/{OUT_DIR}", self.short)
    }

    /// [`Self::inbox`] relative to the data directory, with `/` separators.
    pub fn in_rel(&self) -> String {
        format!("{SESSIONS_DIR}/{}/{IN_DIR}", self.short)
    }
}

/// The folder in `in/` that `id` sent into, looked up by name in `members`; a member that is
/// not listed has an empty name. Example: `bob-3fa21c07`.
pub fn member_dir_in(members: &[Member], id: MemberId) -> String {
    let name = members
        .iter()
        .find(|m| m.id == id)
        .map_or("", |m| m.name.as_str());
    member_dir(name, id)
}

/// The folder in `in/` that holds what `id` sent: their name, made safe for a path, and the
/// short id, which keeps it apart from a namesake. Example: `bob-3fa21c07`.
fn member_dir(name: &str, id: MemberId) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = caps::cut(safe.trim_start_matches('.'), MEMBER_DIR_NAME);
    let safe = if safe.is_empty() { "member" } else { safe };
    format!("{safe}-{}", id.short())
}

/// Bytes of one out file to send.
#[derive(Debug, PartialEq, Eq)]
pub struct Chunk {
    pub stream: String,
    pub offset: u64,
    pub data: Vec<u8>,
}

/// What of each out file has been sent.
#[derive(Default)]
pub struct Outbox {
    offsets: HashMap<String, u64>,
}

/// The regular files in `out` whose names can be sent as a stream, with their sizes, ordered
/// by name; empty when the folder is missing.
pub fn out_files(out: &Path) -> io::Result<Vec<(String, u64)>> {
    let entries = match fs::read_dir(out) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut files: Vec<(String, u64)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !valid_stream_name(&name) {
            continue;
        }
        let meta = entry.metadata()?;
        if meta.is_file() {
            files.push((name, meta.len()));
        }
    }
    files.sort();
    Ok(files)
}

impl Outbox {
    /// The next unsent bytes of every file in `out`, at most [`caps::DEBUG_CHUNK`] per file,
    /// ordered by name. A file that shrank was truncated and is sent again from its start.
    pub fn collect(&mut self, out: &Path) -> io::Result<Vec<Chunk>> {
        let files = out_files(out)?;
        let mut chunks = Vec::new();
        for (stream, len) in files {
            let offset = self.offsets.entry(stream.clone()).or_default();
            if len < *offset {
                *offset = 0;
            }
            if len == *offset {
                continue;
            }
            let want = (len - *offset).min(caps::DEBUG_CHUNK as u64);
            let mut file = File::open(out.join(&stream))?;
            file.seek(SeekFrom::Start(*offset))?;
            let mut data = Vec::with_capacity(want as usize);
            file.take(want).read_to_end(&mut data)?;
            if !data.is_empty() {
                chunks.push(Chunk {
                    stream,
                    offset: *offset,
                    data,
                });
            }
        }
        Ok(chunks)
    }

    /// `stream` was sent up to byte `next`.
    pub fn commit(&mut self, stream: &str, next: u64) {
        self.offsets.insert(stream.to_owned(), next);
    }
}

/// What of each received file has been written.
#[derive(Default)]
pub struct Inbox {
    next: HashMap<(MemberId, String), u64>,
    size: HashMap<(MemberId, String), u64>,
}

impl Inbox {
    /// Writes bytes `offset..` of `from`'s `stream` into `file`; returns the bytes written.
    /// Offset 0 after data means the sender's file started over; a retried chunk is not
    /// written twice; a gap (joined late) is appended anyway.
    pub fn write(
        &mut self,
        file: &Path,
        from: MemberId,
        stream: &str,
        offset: u64,
        data: &[u8],
    ) -> io::Result<u64> {
        let key = (from, stream.to_owned());
        let next = self.next.get(&key).copied().unwrap_or(0);
        let restart = offset == 0 && next > 0;
        let skip = if restart {
            0
        } else {
            next.saturating_sub(offset).min(data.len() as u64) as usize
        };
        self.next.insert(key.clone(), offset + data.len() as u64);
        let size = self.size.entry(key).or_default();
        if restart {
            *size = 0;
        }
        let room = MAX_IN_FILE.saturating_sub(*size) as usize;
        let data = &data[skip..];
        let data = &data[..data.len().min(room)];
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut f = if restart {
            File::create(file)?
        } else {
            OpenOptions::new().create(true).append(true).open(file)?
        };
        if data.is_empty() {
            return Ok(0);
        }
        f.write_all(data)?;
        *size += data.len() as u64;
        Ok(data.len() as u64)
    }
}

/// Removes the session folders under `root` untouched for [`KEEP_FOR`], except `keep`'s.
pub fn prune(root: &Path, keep: DebugId, now: SystemTime) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_str() == Some(&keep.short()) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age > KEEP_FOR);
        if old && let Err(e) = fs::remove_dir_all(entry.path()) {
            tracing::warn!("removing old debug folder {}: {e}", entry.path().display());
        }
    }
}

/// What the worker of one session moved so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traffic {
    pub room: RoomId,
    /// Bytes sent per out file.
    pub sent: BTreeMap<String, u64>,
    /// Bytes received per `<member dir>/<stream>`.
    pub received: BTreeMap<String, u64>,
    /// Received data was lost because this machine fell behind.
    pub lost: bool,
    /// The last error of the worker: a refused send, an out folder it cannot read or an in
    /// folder it cannot write.
    pub error: Option<String>,
}

impl Traffic {
    fn new(room: RoomId) -> Self {
        Self {
            room,
            sent: BTreeMap::new(),
            received: BTreeMap::new(),
            lost: false,
            error: None,
        }
    }
}

/// Identity of a worker: one per room, session and data directory.
#[derive(Clone)]
struct DebugKey {
    client: Client,
    data_dir: PathBuf,
    room: RoomId,
    session: DebugId,
}

impl Hash for DebugKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        0x6462_6773_u32.hash(h);
        self.room.hash(h);
        self.session.hash(h);
        self.data_dir.hash(h);
    }
}

/// The worker of a joined session: sends the out folder, writes what arrives into the in
/// folder and reports its [`Traffic`]. It stops when the subscription is dropped.
pub fn subscription(
    client: Client,
    data_dir: PathBuf,
    room: RoomId,
    session: DebugId,
) -> Subscription<Traffic> {
    Subscription::run_with(
        DebugKey {
            client,
            data_dir,
            room,
            session,
        },
        work,
    )
}

fn work(key: &DebugKey) -> impl Stream<Item = Traffic> + use<> {
    let key = key.clone();
    iced::stream::channel(4, async move |mut out| {
        let DebugKey {
            client,
            data_dir,
            room,
            session,
        } = key;
        let folders = SessionDirs::new(&data_dir, session);
        let (out_dir, in_dir) = (folders.out(), folders.inbox());
        {
            let (out_dir, in_dir) = (out_dir.clone(), in_dir.clone());
            let root = folders.sessions().to_owned();
            let made = tokio::task::spawn_blocking(move || {
                prune(&root, session, SystemTime::now());
                fs::create_dir_all(&out_dir).and_then(|()| fs::create_dir_all(&in_dir))
            })
            .await;
            match made {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!("creating the debug folders: {e}"),
                Err(e) => tracing::warn!("creating the debug folders failed: {e}"),
            }
        }
        let mut rx = client.subscribe();
        let mut tick = tokio::time::interval(SEND_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut outbox = Outbox::default();
        let mut inbox = Inbox::default();
        let mut dirs: HashMap<MemberId, String> = HashMap::new();
        let mut traffic = Traffic::new(room);
        let mut reported = traffic.clone();
        loop {
            tokio::select! {
                event = rx.recv() => match event {
                    Ok(ClientEvent::DebugLog { room: r, session: s, from, stream, offset, data })
                        if r == room && s == session =>
                    {
                        let dir = dirs
                            .entry(from)
                            .or_insert_with(|| member_dir_in(&room_members(&client, room), from))
                            .clone();
                        if !valid_stream_name(&stream) {
                            continue;
                        }
                        let label = format!("{dir}/{stream}");
                        let file = in_dir.join(&dir).join(&stream);
                        let mut ib = std::mem::take(&mut inbox);
                        let written = tokio::task::spawn_blocking(move || {
                            let n = ib.write(&file, from, &stream, offset, &data);
                            (ib, n)
                        })
                        .await;
                        let (ib, n) = match written {
                            Ok(done) => done,
                            Err(e) => {
                                tracing::error!("writing debug log failed: {e}");
                                give_up(&mut out, &mut traffic, "Writing the received debug files failed unexpectedly").await;
                                return;
                            }
                        };
                        inbox = ib;
                        match n {
                            Ok(n) => {
                                *traffic.received.entry(label).or_default() += n;
                            }
                            Err(e) => {
                                tracing::warn!("writing debug log: {e}");
                                traffic.error = Some(format!("Cannot write the received files: {e}"));
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!("debug worker fell behind, {n} events lost");
                        traffic.lost = true;
                    }
                    Err(RecvError::Closed) => return,
                },
                _ = tick.tick() => {
                    let dir = out_dir.clone();
                    let mut ob = std::mem::take(&mut outbox);
                    let collected = tokio::task::spawn_blocking(move || {
                        let chunks = ob.collect(&dir);
                        (ob, chunks)
                    })
                    .await;
                    let (mut ob, chunks) = match collected {
                        Ok(done) => done,
                        Err(e) => {
                            tracing::error!("reading the debug out folder failed: {e}");
                            give_up(&mut out, &mut traffic, "Reading the out folder failed unexpectedly").await;
                            return;
                        }
                    };
                    match chunks {
                        Ok(chunks) => send(&client, room, session, &mut ob, chunks, &mut traffic).await,
                        Err(e) => {
                            tracing::warn!("reading the debug out folder: {e}");
                            traffic.error = Some(format!("Cannot read the out folder: {e}"));
                        }
                    }
                    outbox = ob;
                }
            }
            if traffic != reported {
                reported = traffic.clone();
                if out.send(traffic.clone()).await.is_err() {
                    return;
                }
            }
        }
    })
}

/// Sends `chunks` one after another; stops at the first refusal, which the next tick retries.
async fn send(
    client: &Client,
    room: RoomId,
    session: DebugId,
    outbox: &mut Outbox,
    chunks: Vec<Chunk>,
    traffic: &mut Traffic,
) {
    for Chunk {
        stream,
        offset,
        data,
    } in chunks
    {
        let len = data.len() as u64;
        match client
            .debug_log(room, session, stream.clone(), offset, data)
            .await
        {
            Ok(()) => {
                outbox.commit(&stream, offset + len);
                *traffic.sent.entry(stream).or_default() += len;
                traffic.error = None;
            }
            Err(ClientError::Api(ApiError::LogBudget(_))) => return,
            Err(e) => {
                traffic.error = Some(e.to_string());
                return;
            }
        }
    }
}

/// The worker cannot go on: reports `why` as the last error, then the caller ends it.
async fn give_up(out: &mut Sender<Traffic>, traffic: &mut Traffic, why: &str) {
    traffic.error = Some(why.to_owned());
    let _ = out.send(traffic.clone()).await;
}

/// The members of `room` as the client knows them, empty when the room is unknown.
fn room_members(client: &Client, room: RoomId) -> Vec<Member> {
    client
        .rooms()
        .into_iter()
        .filter_map(|(_, state)| state)
        .find(|state| state.id == room)
        .map(|state| state.members)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> MemberId {
        MemberId([n; 32])
    }

    #[test]
    fn member_dirs_are_safe_and_carry_the_short_id() {
        let bob = id(0x3f);
        assert_eq!(member_dir("Bob Ö", bob), "Bob__-3f3f3f3f");
        assert_eq!(member_dir("..", bob), "member-3f3f3f3f");
        assert_eq!(member_dir(&"x".repeat(40), bob).len(), 32 + 9);
    }

    #[test]
    fn the_outbox_sends_in_chunks_and_restarts_a_truncated_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app.log");
        fs::write(&file, vec![b'a'; caps::DEBUG_CHUNK + 10]).unwrap();
        fs::write(dir.path().join(".hidden"), b"x").unwrap();
        let mut outbox = Outbox::default();
        let chunks = outbox.collect(dir.path()).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data.len(), caps::DEBUG_CHUNK);
        outbox.commit("app.log", caps::DEBUG_CHUNK as u64);
        let chunks = outbox.collect(dir.path()).unwrap();
        assert_eq!(
            (chunks[0].offset, chunks[0].data.len()),
            (caps::DEBUG_CHUNK as u64, 10)
        );
        outbox.commit("app.log", caps::DEBUG_CHUNK as u64 + 10);
        assert!(outbox.collect(dir.path()).unwrap().is_empty());

        fs::write(&file, b"new\n").unwrap();
        let chunks = outbox.collect(dir.path()).unwrap();
        assert_eq!(
            chunks,
            [Chunk {
                stream: "app.log".into(),
                offset: 0,
                data: b"new\n".to_vec()
            }]
        );
    }

    #[test]
    fn the_inbox_skips_retries_restarts_on_zero_and_appends_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bob-1").join("app.log");
        let mut inbox = Inbox::default();
        assert_eq!(inbox.write(&file, id(1), "app.log", 0, b"abc").unwrap(), 3);
        // Bytes 1..4 again, overlapping by two.
        assert_eq!(inbox.write(&file, id(1), "app.log", 1, b"bcd").unwrap(), 1);
        assert_eq!(fs::read(&file).unwrap(), b"abcd");
        // A gap: joined late or lost, appended anyway.
        assert_eq!(inbox.write(&file, id(1), "app.log", 10, b"xy").unwrap(), 2);
        assert_eq!(fs::read(&file).unwrap(), b"abcdxy");
        // The sender's file started over.
        assert_eq!(inbox.write(&file, id(1), "app.log", 0, b"new").unwrap(), 3);
        assert_eq!(fs::read(&file).unwrap(), b"new");
    }
}
