//! SQLite store: rooms, projects, members, tasks and notes.
//!
//! The functions below take a plain `&Connection`; the caller wraps each
//! mutation in one transaction, which is what makes claim selection plus the
//! update atomic.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};
use schwitzer_proto::{
    ApiError, Claim, ClaimToken, Decision, Member, MemberId, Note, Priority, Project, RoomId,
    RoomState, Stage, Task, Work,
};

/// Version stored in `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 3;

const SCHEMA: &str = "
CREATE TABLE people(member BLOB PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE rooms(id BLOB PRIMARY KEY, name TEXT NOT NULL, created_ms INTEGER NOT NULL,
    next_number INTEGER NOT NULL DEFAULT 1, next_project INTEGER NOT NULL DEFAULT 1);
CREATE TABLE projects(room BLOB NOT NULL, id INTEGER NOT NULL, name TEXT NOT NULL,
    created_ms INTEGER NOT NULL, PRIMARY KEY(room, id));
CREATE TABLE members(room BLOB NOT NULL, member BLOB NOT NULL, joined_ms INTEGER NOT NULL,
    PRIMARY KEY(room, member));
CREATE INDEX members_by_member ON members(member);
CREATE TABLE tasks(room BLOB NOT NULL, number INTEGER NOT NULL, project INTEGER NOT NULL,
    title TEXT NOT NULL, body TEXT NOT NULL, plan TEXT NOT NULL, stage INTEGER NOT NULL,
    priority INTEGER NOT NULL, author BLOB NOT NULL, created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL, claim_member BLOB, claim_agent TEXT, claim_work INTEGER,
    claim_since_ms INTEGER, claim_token BLOB, PRIMARY KEY(room, number));
CREATE TABLE notes(room BLOB NOT NULL, number INTEGER NOT NULL, at_ms INTEGER NOT NULL,
    author BLOB NOT NULL, agent TEXT, text TEXT NOT NULL);
CREATE INDEX notes_by_task ON notes(room, number);
CREATE TABLE waits(room BLOB NOT NULL, number INTEGER NOT NULL, after INTEGER NOT NULL,
    PRIMARY KEY(room, number, after));
";

/// From schema 1 (before projects): every room gets project 1, named like the room, holding
/// all its tasks. Room names are capped like project names, so they fit.
const MIGRATE_1: &str = "
ALTER TABLE rooms ADD COLUMN next_project INTEGER NOT NULL DEFAULT 1;
CREATE TABLE projects(room BLOB NOT NULL, id INTEGER NOT NULL, name TEXT NOT NULL,
    created_ms INTEGER NOT NULL, PRIMARY KEY(room, id));
INSERT INTO projects(room, id, name, created_ms) SELECT id, 1, name, created_ms FROM rooms;
UPDATE rooms SET next_project = 2;
ALTER TABLE tasks ADD COLUMN project INTEGER NOT NULL DEFAULT 1;
";

/// From schema 2: the tasks a task waits for.
const MIGRATE_2: &str = "
CREATE TABLE waits(room BLOB NOT NULL, number INTEGER NOT NULL, after INTEGER NOT NULL,
    PRIMARY KEY(room, number, after));
";

/// Why a store closure failed.
#[derive(Debug)]
pub enum Fail {
    /// Return this to the caller.
    Api(ApiError),
    /// Log and answer `Internal`.
    Db(rusqlite::Error),
}

impl From<ApiError> for Fail {
    fn from(e: ApiError) -> Self {
        Fail::Api(e)
    }
}

impl From<rusqlite::Error> for Fail {
    fn from(e: rusqlite::Error) -> Self {
        Fail::Db(e)
    }
}

impl Fail {
    fn into_api(self) -> ApiError {
        match self {
            Fail::Api(e) => e,
            Fail::Db(e) => {
                tracing::error!("database error: {e}");
                ApiError::Internal
            }
        }
    }
}

/// Result inside store closures.
pub type R<T> = Result<T, Fail>;

/// Shared database handle: one connection behind a mutex.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    /// Opens the database file, creating the schema if it is new.
    pub fn open(path: &Path) -> anyhow::Result<Store> {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::init(conn)
    }

    /// Applies the pragmas, then creates, migrates or checks the schema.
    fn init(mut conn: Connection) -> anyhow::Result<Store> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        match version {
            0 => {
                conn.execute_batch(SCHEMA).context("applying schema")?;
                conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            1 | 2 => {
                let tx = conn.transaction()?;
                if version == 1 {
                    tx.execute_batch(MIGRATE_1)
                        .context("migrating from schema version 1")?;
                }
                tx.execute_batch(MIGRATE_2)
                    .context("migrating from schema version 2")?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
                tx.commit()?;
                tracing::info!(
                    "migrated the database from schema version {version} to {SCHEMA_VERSION}"
                );
            }
            SCHEMA_VERSION => {}
            other => bail!("unsupported database schema version {other}"),
        }
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` with the connection on a blocking thread.
    pub async fn run<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> R<T> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        let joined = tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            f(&mut guard)
        })
        .await;
        match joined {
            Ok(r) => r.map_err(Fail::into_api),
            Err(e) => {
                tracing::error!("store task failed: {e}");
                Err(ApiError::Internal)
            }
        }
    }
}

// Explicit integer mappings, in declaration order from 0. They are stored, so
// they never follow the enums' postcard encoding.

fn stage_to_int(stage: Stage) -> i64 {
    match stage {
        Stage::Proposed => 0,
        Stage::Accepted => 1,
        Stage::Planned => 2,
        Stage::Ready => 3,
        Stage::Done => 4,
        Stage::Rejected => 5,
        Stage::Idea => 6,
    }
}

fn stage_from_int(v: i64) -> Option<Stage> {
    Some(match v {
        0 => Stage::Proposed,
        1 => Stage::Accepted,
        2 => Stage::Planned,
        3 => Stage::Ready,
        4 => Stage::Done,
        5 => Stage::Rejected,
        6 => Stage::Idea,
        _ => return None,
    })
}

fn priority_to_int(priority: Priority) -> i64 {
    match priority {
        Priority::High => 0,
        Priority::Normal => 1,
        Priority::Low => 2,
    }
}

fn priority_from_int(v: i64) -> Option<Priority> {
    Some(match v {
        0 => Priority::High,
        1 => Priority::Normal,
        2 => Priority::Low,
        _ => return None,
    })
}

fn work_to_int(work: Work) -> i64 {
    match work {
        Work::Plan => 0,
        Work::Build => 1,
    }
}

fn work_from_int(v: i64) -> Option<Work> {
    Some(match v {
        0 => Work::Plan,
        1 => Work::Build,
        _ => return None,
    })
}

/// Milliseconds or a task number as SQLite integer.
fn int(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// SQLite integer as unsigned (negative values clamp to 0).
fn uns(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

/// An id column holding raw bytes.
trait FromBlob: Sized {
    fn from_blob(bytes: &[u8]) -> Option<Self>;
}

impl FromBlob for MemberId {
    fn from_blob(bytes: &[u8]) -> Option<Self> {
        Self::from_slice(bytes)
    }
}

impl FromBlob for RoomId {
    fn from_blob(bytes: &[u8]) -> Option<Self> {
        Self::from_slice(bytes)
    }
}

impl FromBlob for ClaimToken {
    fn from_blob(bytes: &[u8]) -> Option<Self> {
        Self::from_slice(bytes)
    }
}

fn corrupt(idx: usize, what: &str, ty: rusqlite::types::Type) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(idx, what.into(), ty)
}

fn blob<T: FromBlob>(row: &Row<'_>, idx: usize) -> rusqlite::Result<T> {
    let v: Vec<u8> = row.get(idx)?;
    T::from_blob(&v).ok_or_else(|| corrupt(idx, "id", rusqlite::types::Type::Blob))
}

fn mapped<T>(idx: usize, what: &str, v: i64, map: fn(i64) -> Option<T>) -> rusqlite::Result<T> {
    map(v).ok_or_else(|| corrupt(idx, what, rusqlite::types::Type::Integer))
}

// ---------------------------------------------------------------- people, rooms

/// Stores the display name of `member`; `true` if it changed or is new.
pub fn upsert_person(conn: &Connection, member: &MemberId, name: &str) -> R<bool> {
    let changed = conn.execute(
        "INSERT INTO people(member, name) VALUES (?1, ?2)
         ON CONFLICT(member) DO UPDATE SET name = excluded.name WHERE name <> excluded.name",
        params![member.as_bytes().as_slice(), name],
    )?;
    Ok(changed > 0)
}

/// Requires that the room exists and `member` belongs to it.
pub fn require_member(conn: &Connection, room: &RoomId, member: &MemberId) -> R<()> {
    let joined: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM members WHERE room = ?1 AND member = ?2",
            params![room.as_bytes().as_slice(), member.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    if joined.is_some() {
        return Ok(());
    }
    if room_exists(conn, room)? {
        Err(ApiError::NotMember.into())
    } else {
        Err(ApiError::NoSuchRoom.into())
    }
}

pub fn room_exists(conn: &Connection, room: &RoomId) -> R<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM rooms WHERE id = ?1",
            [room.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Inserts a room and makes `member` its first member.
pub fn create_room(
    conn: &Connection,
    room: &RoomId,
    name: &str,
    member: &MemberId,
    now: u64,
) -> R<()> {
    conn.execute(
        "INSERT INTO rooms(id, name, created_ms) VALUES (?1, ?2, ?3)",
        params![room.as_bytes().as_slice(), name, int(now)],
    )?;
    add_member(conn, room, member, now)?;
    Ok(())
}

/// Adds a membership; `false` if it already existed.
pub fn add_member(conn: &Connection, room: &RoomId, member: &MemberId, now: u64) -> R<bool> {
    let added = conn.execute(
        "INSERT OR IGNORE INTO members(room, member, joined_ms) VALUES (?1, ?2, ?3)",
        params![
            room.as_bytes().as_slice(),
            member.as_bytes().as_slice(),
            int(now)
        ],
    )?;
    Ok(added > 0)
}

/// Removes a membership; the room and its tasks stay.
pub fn remove_member(conn: &Connection, room: &RoomId, member: &MemberId) -> R<()> {
    conn.execute(
        "DELETE FROM members WHERE room = ?1 AND member = ?2",
        params![room.as_bytes().as_slice(), member.as_bytes().as_slice()],
    )?;
    Ok(())
}

/// The rooms `member` belongs to, oldest membership first.
pub fn rooms_of(conn: &Connection, member: &MemberId) -> R<Vec<RoomId>> {
    let mut stmt =
        conn.prepare("SELECT room FROM members WHERE member = ?1 ORDER BY joined_ms, room")?;
    let rooms = stmt
        .query_map([member.as_bytes().as_slice()], |r| blob(r, 0))?
        .collect::<rusqlite::Result<Vec<RoomId>>>()?;
    Ok(rooms)
}

/// How many rooms `member` belongs to.
pub fn membership_count(conn: &Connection, member: &MemberId) -> R<u64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM members WHERE member = ?1",
        [member.as_bytes().as_slice()],
        |r| r.get(0),
    )?;
    Ok(uns(n))
}

pub fn room_member_ids(conn: &Connection, room: &RoomId) -> R<Vec<MemberId>> {
    let mut stmt =
        conn.prepare("SELECT member FROM members WHERE room = ?1 ORDER BY joined_ms, member")?;
    let ids = stmt
        .query_map([room.as_bytes().as_slice()], |r| blob(r, 0))?
        .collect::<rusqlite::Result<Vec<MemberId>>>()?;
    Ok(ids)
}

/// The display name of `member`; empty if it never said hello.
pub fn person_name(conn: &Connection, member: &MemberId) -> R<String> {
    let name: Option<String> = conn
        .query_row(
            "SELECT name FROM people WHERE member = ?1",
            [member.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(name.unwrap_or_default())
}

/// A room with its members (all offline), tasks and no sessions; the caller
/// fills in presence and sessions.
pub fn room_state(conn: &Connection, room: &RoomId) -> R<RoomState> {
    let name: String = conn
        .query_row(
            "SELECT name FROM rooms WHERE id = ?1",
            [room.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(ApiError::NoSuchRoom)?;
    let mut stmt = conn.prepare(
        "SELECT m.member, COALESCE(p.name, '') FROM members m
         LEFT JOIN people p ON p.member = m.member
         WHERE m.room = ?1 ORDER BY m.joined_ms, m.member",
    )?;
    let members = stmt
        .query_map([room.as_bytes().as_slice()], |r| {
            Ok(Member {
                id: blob(r, 0)?,
                name: r.get(1)?,
                online: false,
            })
        })?
        .collect::<rusqlite::Result<Vec<Member>>>()?;
    Ok(RoomState {
        id: *room,
        name,
        members,
        projects: read_projects(conn, room)?,
        tasks: read_tasks(conn, room)?,
        sessions: Vec::new(),
        debug: None,
    })
}

// ------------------------------------------------------------------- projects

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: uns(row.get(0)?),
        name: row.get(1)?,
    })
}

/// Every project of a room, by id.
pub fn read_projects(conn: &Connection, room: &RoomId) -> R<Vec<Project>> {
    let mut stmt = conn.prepare("SELECT id, name FROM projects WHERE room = ?1 ORDER BY id")?;
    let projects = stmt
        .query_map([room.as_bytes().as_slice()], project_from_row)?
        .collect::<rusqlite::Result<Vec<Project>>>()?;
    Ok(projects)
}

/// The project, or `NoSuchProject`.
fn require_project(conn: &Connection, room: &RoomId, id: u64) -> R<Project> {
    let project = conn
        .query_row(
            "SELECT id, name FROM projects WHERE room = ?1 AND id = ?2",
            params![room.as_bytes().as_slice(), int(id)],
            project_from_row,
        )
        .optional()?;
    Ok(project.ok_or(ApiError::NoSuchProject)?)
}

/// Whether another project than `except` has `name`, ignoring case.
fn name_taken(conn: &Connection, room: &RoomId, name: &str, except: Option<u64>) -> R<bool> {
    // Ids start at 1, so 0 excludes nothing.
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM projects WHERE room = ?1 AND name = ?2 COLLATE NOCASE AND id != ?3",
            params![room.as_bytes().as_slice(), name, except.map_or(0, int)],
            |r| r.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// A new project with the room's next project id; its name is unique in the room.
pub fn create_project(conn: &Connection, room: &RoomId, name: &str, now: u64) -> R<Project> {
    if name_taken(conn, room, name, None)? {
        return Err(ApiError::ProjectExists.into());
    }
    let id: i64 = conn.query_row(
        "SELECT next_project FROM rooms WHERE id = ?1",
        [room.as_bytes().as_slice()],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE rooms SET next_project = next_project + 1 WHERE id = ?1",
        [room.as_bytes().as_slice()],
    )?;
    conn.execute(
        "INSERT INTO projects(room, id, name, created_ms) VALUES (?1, ?2, ?3, ?4)",
        params![room.as_bytes().as_slice(), id, name, int(now)],
    )?;
    Ok(Project {
        id: uns(id),
        name: name.to_owned(),
    })
}

/// Renames a project; the new name is unique in the room.
pub fn rename_project(conn: &Connection, room: &RoomId, id: u64, name: &str) -> R<Project> {
    require_project(conn, room, id)?;
    if name_taken(conn, room, name, Some(id))? {
        return Err(ApiError::ProjectExists.into());
    }
    conn.execute(
        "UPDATE projects SET name = ?3 WHERE room = ?1 AND id = ?2",
        params![room.as_bytes().as_slice(), int(id), name],
    )?;
    Ok(Project {
        id,
        name: name.to_owned(),
    })
}

/// Deletes a project that has no tasks in any stage. `rooms.next_project` stays as it is,
/// so the id is never given to another project.
pub fn delete_project(conn: &Connection, room: &RoomId, id: u64) -> R<()> {
    require_project(conn, room, id)?;
    let used: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE room = ?1 AND project = ?2)",
        params![room.as_bytes().as_slice(), int(id)],
        |r| r.get(0),
    )?;
    if used {
        return Err(ApiError::ProjectNotEmpty.into());
    }
    conn.execute(
        "DELETE FROM projects WHERE room = ?1 AND id = ?2",
        params![room.as_bytes().as_slice(), int(id)],
    )?;
    Ok(())
}

/// Moves a task into another project; any stage may move, so mixed rooms can be sorted out.
pub fn move_task(conn: &Connection, room: &RoomId, number: u64, project: u64, now: u64) -> R<Task> {
    require_task(conn, room, number)?;
    require_project(conn, room, project)?;
    conn.execute(
        "UPDATE tasks SET project = ?3, updated_ms = ?4 WHERE room = ?1 AND number = ?2",
        params![
            room.as_bytes().as_slice(),
            int(number),
            int(project),
            int(now)
        ],
    )?;
    require_task(conn, room, number)
}

// ---------------------------------------------------------------------- tasks

const TASK_COLUMNS: &str = "number, title, body, plan, stage, priority, author, created_ms, \
     updated_ms, claim_member, claim_agent, claim_work, claim_since_ms, claim_token, project";

/// Decodes a task row of [`TASK_COLUMNS`]; the notes and waits are filled in by the caller.
fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    let claim_member: Option<Vec<u8>> = row.get(9)?;
    let claim = match claim_member {
        None => None,
        Some(member) => {
            let agent: Option<String> = row.get(10)?;
            let work: Option<i64> = row.get(11)?;
            let since: Option<i64> = row.get(12)?;
            let token: Option<Vec<u8>> = row.get(13)?;
            let bad = |idx| corrupt(idx, "claim", rusqlite::types::Type::Null);
            Some(Claim {
                member: MemberId::from_slice(&member)
                    .ok_or_else(|| corrupt(9, "id", rusqlite::types::Type::Blob))?,
                agent: agent.ok_or_else(|| bad(10))?,
                work: mapped(11, "work", work.ok_or_else(|| bad(11))?, work_from_int)?,
                since_ms: uns(since.ok_or_else(|| bad(12))?),
                token: token
                    .as_deref()
                    .and_then(ClaimToken::from_slice)
                    .ok_or_else(|| bad(13))?,
            })
        }
    };
    Ok(Task {
        number: uns(row.get(0)?),
        title: row.get(1)?,
        body: row.get(2)?,
        plan: row.get(3)?,
        stage: mapped(4, "stage", row.get(4)?, stage_from_int)?,
        priority: mapped(5, "priority", row.get(5)?, priority_from_int)?,
        author: blob(row, 6)?,
        created_ms: uns(row.get(7)?),
        updated_ms: uns(row.get(8)?),
        project: uns(row.get(14)?),
        claim,
        notes: Vec::new(),
        waits_for: Vec::new(),
    })
}

fn note_from_row(row: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        at_ms: uns(row.get(0)?),
        author: blob(row, 1)?,
        agent: row.get(2)?,
        text: row.get(3)?,
    })
}

/// One task with its notes and waits, `None` if the room has no such number.
pub fn read_task(conn: &Connection, room: &RoomId, number: u64) -> R<Option<Task>> {
    let task = conn
        .query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE room = ?1 AND number = ?2"),
            params![room.as_bytes().as_slice(), int(number)],
            task_from_row,
        )
        .optional()?;
    let Some(mut task) = task else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT at_ms, author, agent, text FROM notes
         WHERE room = ?1 AND number = ?2 ORDER BY rowid",
    )?;
    task.notes = stmt
        .query_map(
            params![room.as_bytes().as_slice(), int(number)],
            note_from_row,
        )?
        .collect::<rusqlite::Result<Vec<Note>>>()?;
    let mut stmt =
        conn.prepare("SELECT after FROM waits WHERE room = ?1 AND number = ?2 ORDER BY after")?;
    task.waits_for = stmt
        .query_map(params![room.as_bytes().as_slice(), int(number)], |r| {
            Ok(uns(r.get(0)?))
        })?
        .collect::<rusqlite::Result<Vec<u64>>>()?;
    Ok(Some(task))
}

/// Every task of a room by number, notes and waits included.
pub fn read_tasks(conn: &Connection, room: &RoomId) -> R<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TASK_COLUMNS} FROM tasks WHERE room = ?1 ORDER BY number"
    ))?;
    let mut tasks = stmt
        .query_map([room.as_bytes().as_slice()], task_from_row)?
        .collect::<rusqlite::Result<Vec<Task>>>()?;
    let mut stmt = conn.prepare(
        "SELECT at_ms, author, agent, text, number FROM notes WHERE room = ?1 ORDER BY rowid",
    )?;
    let mut notes: HashMap<u64, Vec<Note>> = HashMap::new();
    for row in stmt.query_map([room.as_bytes().as_slice()], |r| {
        Ok((uns(r.get(4)?), note_from_row(r)?))
    })? {
        let (number, note) = row?;
        notes.entry(number).or_default().push(note);
    }
    let mut stmt =
        conn.prepare("SELECT number, after FROM waits WHERE room = ?1 ORDER BY number, after")?;
    let mut waits: HashMap<u64, Vec<u64>> = HashMap::new();
    for row in stmt.query_map([room.as_bytes().as_slice()], |r| {
        Ok((uns(r.get(0)?), uns(r.get(1)?)))
    })? {
        let (number, after) = row?;
        waits.entry(number).or_default().push(after);
    }
    for task in &mut tasks {
        task.notes = notes.remove(&task.number).unwrap_or_default();
        task.waits_for = waits.remove(&task.number).unwrap_or_default();
    }
    Ok(tasks)
}

/// The task, or `NoSuchTask`.
fn require_task(conn: &Connection, room: &RoomId, number: u64) -> R<Task> {
    Ok(read_task(conn, room, number)?.ok_or(ApiError::NoSuchTask)?)
}

/// `WrongStage` once the task is `Done` or `Rejected`.
fn require_open(task: &Task) -> R<()> {
    if matches!(task.stage, Stage::Done | Stage::Rejected) {
        return Err(ApiError::WrongStage(task.stage).into());
    }
    Ok(())
}

fn append_note(
    conn: &Connection,
    room: &RoomId,
    number: u64,
    author: &MemberId,
    agent: Option<&str>,
    text: &str,
    now: u64,
) -> R<()> {
    conn.execute(
        "INSERT INTO notes(room, number, at_ms, author, agent, text) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            room.as_bytes().as_slice(),
            int(number),
            int(now),
            author.as_bytes().as_slice(),
            agent,
            text
        ],
    )?;
    Ok(())
}

fn touch(conn: &Connection, room: &RoomId, number: u64, now: u64) -> R<()> {
    conn.execute(
        "UPDATE tasks SET updated_ms = ?3 WHERE room = ?1 AND number = ?2",
        params![room.as_bytes().as_slice(), int(number), int(now)],
    )?;
    Ok(())
}

fn clear_claim(conn: &Connection, room: &RoomId, number: u64) -> R<()> {
    conn.execute(
        "UPDATE tasks SET claim_member = NULL, claim_agent = NULL, claim_work = NULL,
             claim_since_ms = NULL, claim_token = NULL WHERE room = ?1 AND number = ?2",
        params![room.as_bytes().as_slice(), int(number)],
    )?;
    Ok(())
}

fn set_stage(conn: &Connection, room: &RoomId, number: u64, stage: Stage, now: u64) -> R<()> {
    conn.execute(
        "UPDATE tasks SET stage = ?3, updated_ms = ?4 WHERE room = ?1 AND number = ?2",
        params![
            room.as_bytes().as_slice(),
            int(number),
            stage_to_int(stage),
            int(now)
        ],
    )?;
    Ok(())
}

/// A new task of `project` in `stage` (`Idea` or `Proposed`) with priority `Normal`.
#[allow(clippy::too_many_arguments)]
pub fn create_task(
    conn: &Connection,
    room: &RoomId,
    author: &MemberId,
    project: u64,
    stage: Stage,
    title: &str,
    body: &str,
    now: u64,
) -> R<Task> {
    require_project(conn, room, project)?;
    let number: i64 = conn.query_row(
        "SELECT next_number FROM rooms WHERE id = ?1",
        [room.as_bytes().as_slice()],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE rooms SET next_number = next_number + 1 WHERE id = ?1",
        [room.as_bytes().as_slice()],
    )?;
    conn.execute(
        "INSERT INTO tasks(room, number, project, title, body, plan, stage, priority, author,
             created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, ?7, ?8, ?9, ?9)",
        params![
            room.as_bytes().as_slice(),
            number,
            int(project),
            title,
            body,
            stage_to_int(stage),
            priority_to_int(Priority::Normal),
            author.as_bytes().as_slice(),
            int(now)
        ],
    )?;
    require_task(conn, room, uns(number))
}

/// Writes an idea up into a proposal: new title and body, stage `Proposed`; the author stays.
/// `waits` is added to what the idea already waits for.
pub fn propose_idea(
    conn: &Connection,
    room: &RoomId,
    number: u64,
    title: &str,
    body: &str,
    waits: &[u64],
    now: u64,
) -> R<Task> {
    let task = require_task(conn, room, number)?;
    if task.stage != Stage::Idea {
        return Err(ApiError::WrongStage(task.stage).into());
    }
    let merged: Vec<u64> = task
        .waits_for
        .iter()
        .chain(waits)
        .copied()
        .collect::<BTreeSet<u64>>()
        .into_iter()
        .collect();
    write_waits(conn, room, number, &merged)?;
    conn.execute(
        "UPDATE tasks SET title = ?3, body = ?4, stage = ?5, updated_ms = ?6
         WHERE room = ?1 AND number = ?2",
        params![
            room.as_bytes().as_slice(),
            int(number),
            title,
            body,
            stage_to_int(Stage::Proposed),
            int(now)
        ],
    )?;
    require_task(conn, room, number)
}

/// Rewrites title, body and plan; not allowed once a task is `Done` or `Rejected`.
pub fn edit_task(
    conn: &Connection,
    room: &RoomId,
    number: u64,
    title: &str,
    body: &str,
    plan: &str,
    now: u64,
) -> R<Task> {
    require_open(&require_task(conn, room, number)?)?;
    conn.execute(
        "UPDATE tasks SET title = ?3, body = ?4, plan = ?5, updated_ms = ?6
         WHERE room = ?1 AND number = ?2",
        params![
            room.as_bytes().as_slice(),
            int(number),
            title,
            body,
            plan,
            int(now)
        ],
    )?;
    require_task(conn, room, number)
}

/// Replaces the tasks `number` waits for; not allowed once it is `Done` or `Rejected`.
/// `waits` is ascending and without duplicates.
pub fn set_waits(
    conn: &Connection,
    room: &RoomId,
    number: u64,
    waits: &[u64],
    now: u64,
) -> R<Task> {
    require_open(&require_task(conn, room, number)?)?;
    write_waits(conn, room, number, waits)?;
    touch(conn, room, number, now)?;
    require_task(conn, room, number)
}

/// Checks `waits` (ascending, without duplicates) for task `number` and replaces its waits with
/// them: no task waits for itself, for a number the room does not have, or for a task that
/// already waits for it, directly or through other tasks.
fn write_waits(conn: &Connection, room: &RoomId, number: u64, waits: &[u64]) -> R<()> {
    let room_bytes = room.as_bytes().as_slice();
    for &after in waits {
        if after == number {
            return Err(ApiError::Invalid("a task cannot wait for itself".into()).into());
        }
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE room = ?1 AND number = ?2)",
            params![room_bytes, int(after)],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(ApiError::Invalid(format!("there is no task #{after}")).into());
        }
        // UNION, not UNION ALL: the walk ends even if the stored waits hold a cycle.
        let cycle: bool = conn.query_row(
            "SELECT EXISTS(WITH RECURSIVE r(n) AS (SELECT ?2 UNION SELECT w.after FROM waits w
                 JOIN r ON w.room = ?1 AND w.number = r.n) SELECT 1 FROM r WHERE n = ?3)",
            params![room_bytes, int(after), int(number)],
            |r| r.get(0),
        )?;
        if cycle {
            return Err(ApiError::Invalid(format!(
                "#{after} already waits for #{number}, directly or through other tasks"
            ))
            .into());
        }
    }
    conn.execute(
        "DELETE FROM waits WHERE room = ?1 AND number = ?2",
        params![room_bytes, int(number)],
    )?;
    for &after in waits {
        conn.execute(
            "INSERT INTO waits(room, number, after) VALUES (?1, ?2, ?3)",
            params![room_bytes, int(number), int(after)],
        )?;
    }
    Ok(())
}

pub fn set_priority(
    conn: &Connection,
    room: &RoomId,
    number: u64,
    priority: Priority,
    now: u64,
) -> R<Task> {
    require_task(conn, room, number)?;
    conn.execute(
        "UPDATE tasks SET priority = ?3, updated_ms = ?4 WHERE room = ?1 AND number = ?2",
        params![
            room.as_bytes().as_slice(),
            int(number),
            priority_to_int(priority),
            int(now)
        ],
    )?;
    require_task(conn, room, number)
}

/// The stage a decision leads to, or the error for an illegal combination.
fn next_stage(decision: Decision, task: &Task) -> Result<Stage, ApiError> {
    let wrong = || ApiError::WrongStage(task.stage);
    match (decision, task.stage) {
        (Decision::Accept, Stage::Proposed) => Ok(Stage::Accepted),
        (Decision::Reject, Stage::Idea | Stage::Proposed | Stage::Accepted | Stage::Planned) => {
            Ok(Stage::Rejected)
        }
        (Decision::Approve, Stage::Planned) => {
            if !task.has_plan() {
                Err(ApiError::Invalid("plan is empty".into()))
            } else {
                Ok(Stage::Ready)
            }
        }
        (Decision::Replan, Stage::Planned) => Ok(Stage::Accepted),
        (Decision::Replan, Stage::Ready) if task.claim.is_none() => Ok(Stage::Accepted),
        (Decision::Reopen, Stage::Rejected | Stage::Done) => Ok(Stage::Proposed),
        (Decision::MarkPlanned, Stage::Accepted) => {
            if !task.has_plan() {
                Err(ApiError::Invalid("plan is empty".into()))
            } else {
                Ok(Stage::Planned)
            }
        }
        (Decision::MarkDone, Stage::Ready) => Ok(Stage::Done),
        _ => Err(wrong()),
    }
}

/// Applies a human decision to the task's stage.
pub fn decide(
    conn: &Connection,
    room: &RoomId,
    number: u64,
    decision: Decision,
    now: u64,
) -> R<Task> {
    let task = require_task(conn, room, number)?;
    let stage = next_stage(decision, &task)?;
    if matches!(
        decision,
        Decision::Reject | Decision::MarkPlanned | Decision::MarkDone
    ) {
        clear_claim(conn, room, number)?;
    }
    set_stage(conn, room, number, stage, now)?;
    require_task(conn, room, number)
}

/// Claims a task for `member`: `Plan` takes from `Accepted`, `Build` from `Ready`.
/// Without a number the first unclaimed task by (priority, number) is taken, from
/// `project` if given, and `None` means there is none. With a number, `project` is ignored.
/// A build skips, or with its number refuses, a task that waits for one not `Done`.
#[allow(clippy::too_many_arguments)]
pub fn claim(
    conn: &Connection,
    room: &RoomId,
    member: &MemberId,
    work: Work,
    project: Option<u64>,
    number: Option<u64>,
    agent: &str,
    now: u64,
) -> R<Option<Task>> {
    let target = match work {
        Work::Plan => Stage::Accepted,
        Work::Build => Stage::Ready,
    };
    let number = match number {
        Some(n) => {
            let task = require_task(conn, room, n)?;
            if task.stage != target {
                return Err(ApiError::WrongStage(task.stage).into());
            }
            if task.claim.is_some() {
                return Err(ApiError::Claimed.into());
            }
            if work == Work::Build {
                let open: Option<i64> = conn.query_row(
                    "SELECT MIN(w.after) FROM waits w
                         JOIN tasks p ON p.room = w.room AND p.number = w.after
                     WHERE w.room = ?1 AND w.number = ?2 AND p.stage != ?3",
                    params![
                        room.as_bytes().as_slice(),
                        int(n),
                        stage_to_int(Stage::Done)
                    ],
                    |r| r.get(0),
                )?;
                if let Some(open) = open {
                    return Err(ApiError::Waiting(uns(open)).into());
                }
            }
            n
        }
        None => {
            if let Some(project) = project {
                require_project(conn, room, project)?;
            }
            let next: Option<i64> = conn
                .query_row(
                    "SELECT number FROM tasks t
                     WHERE room = ?1 AND stage = ?2 AND claim_member IS NULL
                         AND (?3 IS NULL OR project = ?3)
                         AND (?4 = 0 OR NOT EXISTS (SELECT 1 FROM waits w
                             JOIN tasks p ON p.room = w.room AND p.number = w.after
                             WHERE w.room = t.room AND w.number = t.number AND p.stage != ?5))
                     ORDER BY priority, number LIMIT 1",
                    params![
                        room.as_bytes().as_slice(),
                        stage_to_int(target),
                        project.map(int),
                        work == Work::Build,
                        stage_to_int(Stage::Done)
                    ],
                    |r| r.get(0),
                )
                .optional()?;
            match next {
                Some(n) => uns(n),
                None => return Ok(None),
            }
        }
    };
    conn.execute(
        "UPDATE tasks SET claim_member = ?3, claim_agent = ?4, claim_work = ?5,
             claim_since_ms = ?6, claim_token = ?7, updated_ms = ?6
         WHERE room = ?1 AND number = ?2",
        params![
            room.as_bytes().as_slice(),
            int(number),
            member.as_bytes().as_slice(),
            agent,
            work_to_int(work),
            int(now),
            ClaimToken::random().as_bytes().as_slice()
        ],
    )?;
    Ok(Some(require_task(conn, room, number)?))
}

/// Stores a plan: by the planner holding the claim, or as a refinement of a
/// `Planned` task by anyone. The task ends up `Planned` and unclaimed.
pub fn submit_plan(
    conn: &Connection,
    room: &RoomId,
    member: &MemberId,
    number: u64,
    plan: &str,
    now: u64,
) -> R<Task> {
    let task = require_task(conn, room, number)?;
    match task.stage {
        Stage::Planned => {}
        Stage::Accepted => {
            let mine = task
                .claim
                .as_ref()
                .is_some_and(|c| c.work == Work::Plan && c.member == *member);
            if !mine {
                return Err(ApiError::NotYourClaim.into());
            }
        }
        stage => return Err(ApiError::WrongStage(stage).into()),
    }
    clear_claim(conn, room, number)?;
    conn.execute(
        "UPDATE tasks SET plan = ?3, stage = ?4, updated_ms = ?5 WHERE room = ?1 AND number = ?2",
        params![
            room.as_bytes().as_slice(),
            int(number),
            plan,
            stage_to_int(Stage::Planned),
            int(now)
        ],
    )?;
    require_task(conn, room, number)
}

/// Marks a built task `Done`; only its builder may.
pub fn finish(
    conn: &Connection,
    room: &RoomId,
    member: &MemberId,
    number: u64,
    note: &str,
    now: u64,
) -> R<Task> {
    let task = require_task(conn, room, number)?;
    if task.stage != Stage::Ready {
        return Err(ApiError::WrongStage(task.stage).into());
    }
    let claim = task
        .claim
        .filter(|c| c.work == Work::Build && c.member == *member)
        .ok_or(ApiError::NotYourClaim)?;
    clear_claim(conn, room, number)?;
    set_stage(conn, room, number, Stage::Done, now)?;
    if !note.is_empty() {
        append_note(conn, room, number, member, Some(&claim.agent), note, now)?;
    }
    require_task(conn, room, number)
}

/// Drops the claim; any member may, so stuck claims can be freed.
pub fn unclaim(
    conn: &Connection,
    room: &RoomId,
    member: &MemberId,
    number: u64,
    note: &str,
    now: u64,
) -> R<Task> {
    let task = require_task(conn, room, number)?;
    let claim = task
        .claim
        .ok_or_else(|| ApiError::Invalid("not claimed".into()))?;
    clear_claim(conn, room, number)?;
    touch(conn, room, number, now)?;
    if !note.is_empty() {
        let agent = (claim.member == *member).then_some(claim.agent.as_str());
        append_note(conn, room, number, member, agent, note, now)?;
    }
    require_task(conn, room, number)
}

pub fn add_note(
    conn: &Connection,
    room: &RoomId,
    member: &MemberId,
    number: u64,
    text: &str,
    agent: Option<&str>,
    now: u64,
) -> R<Task> {
    require_task(conn, room, number)?;
    append_note(conn, room, number, member, agent, text, now)?;
    touch(conn, room, number, now)?;
    require_task(conn, room, number)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000;

    fn member(n: u8) -> MemberId {
        MemberId([n; 32])
    }

    /// The connection of a store that nothing else holds.
    fn into_conn(store: Store) -> Connection {
        Arc::try_unwrap(store.conn)
            .ok()
            .unwrap()
            .into_inner()
            .unwrap()
    }

    /// A store connection with one room, its project 1 ("app") and member 1 in it.
    fn setup() -> (Connection, RoomId, MemberId) {
        let conn = into_conn(Store::init(Connection::open_in_memory().unwrap()).unwrap());
        let room = RoomId([7; 32]);
        let me = member(1);
        create_room(&conn, &room, "room", &me, NOW).unwrap();
        create_project(&conn, &room, "app", NOW).unwrap();
        (conn, room, me)
    }

    /// Creates a task and walks it to `stage` (Idea, Proposed, Accepted, Planned or Ready).
    fn task_in(conn: &Connection, room: &RoomId, me: &MemberId, stage: Stage) -> u64 {
        let first = if stage == Stage::Idea {
            Stage::Idea
        } else {
            Stage::Proposed
        };
        let n = create_task(conn, room, me, 1, first, "t", "", NOW)
            .unwrap()
            .number;
        let steps: &[Decision] = match stage {
            Stage::Idea | Stage::Proposed => &[],
            Stage::Accepted => &[Decision::Accept],
            Stage::Planned => &[Decision::Accept],
            Stage::Ready => &[Decision::Accept],
            _ => unreachable!("not needed by the tests"),
        };
        for d in steps {
            decide(conn, room, n, *d, NOW).unwrap();
        }
        if matches!(stage, Stage::Planned | Stage::Ready) {
            claim(conn, room, me, Work::Plan, None, Some(n), "a", NOW).unwrap();
            submit_plan(conn, room, me, n, "plan", NOW).unwrap();
        }
        if stage == Stage::Ready {
            decide(conn, room, n, Decision::Approve, NOW).unwrap();
        }
        n
    }

    #[test]
    fn claim_without_number_takes_priority_then_number() {
        let (conn, room, me) = setup();
        let nums: Vec<u64> = (0..4)
            .map(|_| task_in(&conn, &room, &me, Stage::Accepted))
            .collect();
        set_priority(&conn, &room, nums[2], Priority::High, NOW).unwrap();
        set_priority(&conn, &room, nums[0], Priority::Low, NOW).unwrap();

        let mut order = Vec::new();
        while let Some(t) = claim(&conn, &room, &me, Work::Plan, None, None, "a", NOW).unwrap() {
            assert_eq!(t.claim.as_ref().unwrap().work, Work::Plan);
            order.push(t.number);
        }
        // High first, then Normal by number, Low last.
        assert_eq!(order, vec![nums[2], nums[1], nums[3], nums[0]]);
    }

    #[test]
    fn claim_targets_the_stage_of_its_work() {
        let (conn, room, me) = setup();
        let accepted = task_in(&conn, &room, &me, Stage::Accepted);
        let ready = task_in(&conn, &room, &me, Stage::Ready);

        let built = claim(&conn, &room, &me, Work::Build, None, None, "a", NOW)
            .unwrap()
            .unwrap();
        assert_eq!(built.number, ready);
        assert!(
            claim(&conn, &room, &me, Work::Build, None, None, "a", NOW)
                .unwrap()
                .is_none()
        );
        let planned = claim(&conn, &room, &me, Work::Plan, None, None, "a", NOW)
            .unwrap()
            .unwrap();
        assert_eq!(planned.number, accepted);
    }

    #[test]
    fn explicit_claim_checks_stage_and_existing_claim() {
        let (conn, room, me) = setup();
        let n = task_in(&conn, &room, &me, Stage::Accepted);
        assert!(matches!(
            claim(&conn, &room, &me, Work::Build, None, Some(n), "a", NOW),
            Err(Fail::Api(ApiError::WrongStage(Stage::Accepted)))
        ));
        claim(&conn, &room, &me, Work::Plan, None, Some(n), "a", NOW).unwrap();
        assert!(matches!(
            claim(
                &conn,
                &room,
                &member(2),
                Work::Plan,
                None,
                Some(n),
                "b",
                NOW
            ),
            Err(Fail::Api(ApiError::Claimed))
        ));
        assert!(matches!(
            claim(&conn, &room, &me, Work::Plan, None, Some(99), "a", NOW),
            Err(Fail::Api(ApiError::NoSuchTask))
        ));
    }

    #[test]
    fn an_idea_is_written_up_into_a_proposal() {
        let (conn, room, me) = setup();
        let n = task_in(&conn, &room, &me, Stage::Idea);
        assert!(matches!(
            claim(&conn, &room, &me, Work::Plan, None, Some(n), "a", NOW),
            Err(Fail::Api(ApiError::WrongStage(Stage::Idea)))
        ));
        let task = propose_idea(&conn, &room, n, "T2", "b2", &[], NOW + 1).unwrap();
        assert_eq!(task.number, n);
        assert_eq!(task.stage, Stage::Proposed);
        assert_eq!((task.title.as_str(), task.body.as_str()), ("T2", "b2"));
        assert_eq!(task.author, me);
        assert!(matches!(
            propose_idea(&conn, &room, n, "T3", "", &[], NOW),
            Err(Fail::Api(ApiError::WrongStage(Stage::Proposed)))
        ));
    }

    /// Runs `decision` on a fresh task prepared by `prepare`; the resulting
    /// stage or the error.
    fn decide_from(
        prepare: impl FnOnce(&Connection, &RoomId, &MemberId) -> u64,
        decision: Decision,
    ) -> Result<Stage, ApiError> {
        let (conn, room, me) = setup();
        let n = prepare(&conn, &room, &me);
        match decide(&conn, &room, n, decision, NOW) {
            Ok(task) => Ok(task.stage),
            Err(Fail::Api(e)) => Err(e),
            Err(Fail::Db(e)) => panic!("database error: {e}"),
        }
    }

    fn at(stage: Stage) -> impl FnOnce(&Connection, &RoomId, &MemberId) -> u64 {
        move |conn, room, me| task_in(conn, room, me, stage)
    }

    #[test]
    fn decide_legal_transitions() {
        use Decision::*;
        use Stage::*;
        assert_eq!(decide_from(at(Proposed), Accept), Ok(Accepted));
        for from in [Idea, Proposed, Accepted, Planned] {
            assert_eq!(decide_from(at(from), Reject), Ok(Rejected));
        }
        assert_eq!(decide_from(at(Planned), Approve), Ok(Ready));
        assert_eq!(decide_from(at(Planned), Replan), Ok(Accepted));
        assert_eq!(decide_from(at(Ready), Replan), Ok(Accepted));
        let rejected = |c: &Connection, r: &RoomId, m: &MemberId| {
            let n = task_in(c, r, m, Proposed);
            decide(c, r, n, Reject, NOW).unwrap();
            n
        };
        assert_eq!(decide_from(rejected, Reopen), Ok(Proposed));
        let done = |c: &Connection, r: &RoomId, m: &MemberId| {
            let n = task_in(c, r, m, Ready);
            claim(c, r, m, Work::Build, None, Some(n), "a", NOW).unwrap();
            finish(c, r, m, n, "", NOW).unwrap();
            n
        };
        assert_eq!(decide_from(done, Reopen), Ok(Proposed));
        assert_eq!(decide_from(at(Ready), MarkDone), Ok(Done));
        let with_plan = |c: &Connection, r: &RoomId, m: &MemberId| {
            let n = task_in(c, r, m, Accepted);
            edit_task(c, r, n, "t", "", "plan", NOW).unwrap();
            n
        };
        assert_eq!(decide_from(with_plan, MarkPlanned), Ok(Planned));
    }

    #[test]
    fn decide_illegal_transitions_are_wrong_stage() {
        use Decision::*;
        use Stage::*;
        let illegal = [
            (Idea, Accept),
            (Idea, Approve),
            (Idea, Replan),
            (Idea, Reopen),
            (Proposed, Approve),
            (Proposed, Replan),
            (Proposed, Reopen),
            (Accepted, Accept),
            (Accepted, Approve),
            (Accepted, Replan),
            (Accepted, Reopen),
            (Planned, Accept),
            (Planned, Reopen),
            (Ready, Accept),
            (Ready, Reject),
            (Ready, Approve),
            (Ready, Reopen),
            (Idea, MarkPlanned),
            (Proposed, MarkPlanned),
            (Planned, MarkPlanned),
            (Ready, MarkPlanned),
            (Idea, MarkDone),
            (Proposed, MarkDone),
            (Accepted, MarkDone),
            (Planned, MarkDone),
        ];
        for (from, decision) in illegal {
            assert_eq!(
                decide_from(at(from), decision),
                Err(ApiError::WrongStage(from)),
                "{decision:?} from {from:?}"
            );
        }
    }

    #[test]
    fn replan_of_a_claimed_ready_task_is_wrong_stage() {
        let claimed = |c: &Connection, r: &RoomId, m: &MemberId| {
            let n = task_in(c, r, m, Stage::Ready);
            claim(c, r, m, Work::Build, None, Some(n), "a", NOW).unwrap();
            n
        };
        assert_eq!(
            decide_from(claimed, Decision::Replan),
            Err(ApiError::WrongStage(Stage::Ready))
        );
    }

    #[test]
    fn approve_needs_a_plan_and_reject_clears_the_claim() {
        let (conn, room, me) = setup();
        let n = task_in(&conn, &room, &me, Stage::Planned);
        edit_task(&conn, &room, n, "t", "", "", NOW).unwrap();
        assert!(matches!(
            decide(&conn, &room, n, Decision::Approve, NOW),
            Err(Fail::Api(ApiError::Invalid(m))) if m == "plan is empty"
        ));

        let n = task_in(&conn, &room, &me, Stage::Accepted);
        claim(&conn, &room, &me, Work::Plan, None, Some(n), "a", NOW).unwrap();
        let task = decide(&conn, &room, n, Decision::Reject, NOW).unwrap();
        assert_eq!(task.stage, Stage::Rejected);
        assert!(task.claim.is_none());
    }

    #[test]
    fn mark_planned_and_mark_done_clear_the_claim() {
        let (conn, room, me) = setup();
        let agent = member(2);
        let n = task_in(&conn, &room, &me, Stage::Accepted);
        claim(&conn, &room, &agent, Work::Plan, None, Some(n), "a", NOW).unwrap();
        edit_task(&conn, &room, n, "t", "", "p", NOW).unwrap();
        let task = decide(&conn, &room, n, Decision::MarkPlanned, NOW).unwrap();
        assert_eq!(task.stage, Stage::Planned);
        assert!(task.claim.is_none());
        assert_eq!(task.plan, "p");

        let n = task_in(&conn, &room, &me, Stage::Accepted);
        assert!(matches!(
            decide(&conn, &room, n, Decision::MarkPlanned, NOW),
            Err(Fail::Api(ApiError::Invalid(m))) if m == "plan is empty"
        ));

        let n = task_in(&conn, &room, &me, Stage::Ready);
        claim(&conn, &room, &agent, Work::Build, None, Some(n), "a", NOW).unwrap();
        let task = decide(&conn, &room, n, Decision::MarkDone, NOW).unwrap();
        assert_eq!(task.stage, Stage::Done);
        assert!(task.claim.is_none());
        assert!(matches!(
            finish(&conn, &room, &agent, n, "", NOW),
            Err(Fail::Api(ApiError::WrongStage(Stage::Done)))
        ));
    }

    #[test]
    fn newer_schema_versions_are_refused() {
        for version in [4, 99, -1] {
            let conn = Connection::open_in_memory().unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
            let err = Store::init(conn).err().expect("refused");
            assert!(
                err.to_string().contains(&version.to_string()),
                "error names the version: {err}"
            );
        }
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        // The current version is used as is.
        assert!(Store::init(conn).is_ok());
    }

    /// Schema version 1 as it was before projects, verbatim.
    const SCHEMA_V1: &str = "
CREATE TABLE people(member BLOB PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE rooms(id BLOB PRIMARY KEY, name TEXT NOT NULL, created_ms INTEGER NOT NULL,
    next_number INTEGER NOT NULL DEFAULT 1);
CREATE TABLE members(room BLOB NOT NULL, member BLOB NOT NULL, joined_ms INTEGER NOT NULL,
    PRIMARY KEY(room, member));
CREATE INDEX members_by_member ON members(member);
CREATE TABLE tasks(room BLOB NOT NULL, number INTEGER NOT NULL, title TEXT NOT NULL,
    body TEXT NOT NULL, plan TEXT NOT NULL, stage INTEGER NOT NULL, priority INTEGER NOT NULL,
    author BLOB NOT NULL, created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL,
    claim_member BLOB, claim_agent TEXT, claim_work INTEGER, claim_since_ms INTEGER,
    claim_token BLOB, PRIMARY KEY(room, number));
CREATE TABLE notes(room BLOB NOT NULL, number INTEGER NOT NULL, at_ms INTEGER NOT NULL,
    author BLOB NOT NULL, agent TEXT, text TEXT NOT NULL);
CREATE INDEX notes_by_task ON notes(room, number);
";

    #[test]
    fn migration_puts_every_room_and_its_tasks_into_project_one() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        let room = RoomId([7; 32]);
        conn.execute(
            "INSERT INTO rooms(id, name, created_ms, next_number) VALUES (?1, 'crew', 5, 2)",
            [room.as_bytes().as_slice()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks(room, number, title, body, plan, stage, priority, author,
                 created_ms, updated_ms) VALUES (?1, 1, 't', '', '', 0, 1, ?2, 5, 5)",
            [room.as_bytes().as_slice(), member(1).as_bytes().as_slice()],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();

        let conn = into_conn(Store::init(conn).unwrap());
        assert_eq!(
            read_projects(&conn, &room).unwrap(),
            vec![Project {
                id: 1,
                name: "crew".into()
            }]
        );
        assert_eq!(read_task(&conn, &room, 1).unwrap().unwrap().project, 1);
        let other = create_task(&conn, &room, &member(1), 1, Stage::Proposed, "o", "", NOW)
            .unwrap()
            .number;
        assert_eq!(
            set_waits(&conn, &room, 1, &[other], NOW).unwrap().waits_for,
            vec![other]
        );
        assert_eq!(create_project(&conn, &room, "web", NOW).unwrap().id, 2);
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn schema_2_gains_the_waits_table() {
        let conn = into_conn(Store::init(Connection::open_in_memory().unwrap()).unwrap());
        conn.execute_batch("DROP TABLE waits; PRAGMA user_version = 2;")
            .unwrap();
        let conn = into_conn(Store::init(conn).unwrap());
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let room = RoomId([7; 32]);
        let me = member(1);
        create_room(&conn, &room, "room", &me, NOW).unwrap();
        create_project(&conn, &room, "app", NOW).unwrap();
        let a = task_in(&conn, &room, &me, Stage::Proposed);
        let b = task_in(&conn, &room, &me, Stage::Proposed);
        assert_eq!(
            set_waits(&conn, &room, b, &[a], NOW).unwrap().waits_for,
            vec![a]
        );
    }

    #[test]
    fn build_claim_skips_a_task_until_what_it_waits_for_is_done() {
        let (conn, room, me) = setup();
        let p = task_in(&conn, &room, &me, Stage::Ready);
        let w = task_in(&conn, &room, &me, Stage::Ready);
        set_waits(&conn, &room, w, &[p], NOW).unwrap();
        let build = |n| claim(&conn, &room, &me, Work::Build, None, n, "a", NOW);

        assert_eq!(build(None).unwrap().unwrap().number, p);
        assert!(build(None).unwrap().is_none());
        assert!(matches!(
            build(Some(w)),
            Err(Fail::Api(ApiError::Waiting(n))) if n == p
        ));
        finish(&conn, &room, &me, p, "done", NOW).unwrap();
        assert_eq!(build(None).unwrap().unwrap().number, w);

        // Planning ignores the waits.
        let q = task_in(&conn, &room, &me, Stage::Accepted);
        set_waits(&conn, &room, q, &[w], NOW).unwrap();
        assert!(claim(&conn, &room, &me, Work::Plan, None, Some(q), "a", NOW).is_ok());
    }

    #[test]
    fn waits_refuse_self_unknown_cycles_and_closed_tasks() {
        let (conn, room, me) = setup();
        let a = task_in(&conn, &room, &me, Stage::Proposed);
        let b = task_in(&conn, &room, &me, Stage::Proposed);
        let c = task_in(&conn, &room, &me, Stage::Proposed);
        let invalid = |r: R<Task>| match r {
            Err(Fail::Api(ApiError::Invalid(text))) => text,
            other => panic!("expected Invalid, got {other:?}"),
        };
        assert_eq!(
            invalid(set_waits(&conn, &room, a, &[a], NOW)),
            "a task cannot wait for itself"
        );
        assert_eq!(
            invalid(set_waits(&conn, &room, a, &[99], NOW)),
            "there is no task #99"
        );
        set_waits(&conn, &room, a, &[b], NOW).unwrap();
        set_waits(&conn, &room, b, &[c], NOW).unwrap();
        assert_eq!(
            invalid(set_waits(&conn, &room, c, &[a], NOW)),
            format!("#{a} already waits for #{c}, directly or through other tasks")
        );

        let d = task_in(&conn, &room, &me, Stage::Proposed);
        set_waits(&conn, &room, d, &[a, c], NOW).unwrap();
        let tasks = read_tasks(&conn, &room).unwrap();
        let waits_of = |n| {
            tasks
                .iter()
                .find(|t| t.number == n)
                .unwrap()
                .waits_for
                .clone()
        };
        assert_eq!(waits_of(d), vec![a, c]);
        assert_eq!(waits_of(a), vec![b]);

        let done = task_in(&conn, &room, &me, Stage::Ready);
        decide(&conn, &room, done, Decision::MarkDone, NOW).unwrap();
        assert!(matches!(
            set_waits(&conn, &room, done, &[a], NOW),
            Err(Fail::Api(ApiError::WrongStage(Stage::Done)))
        ));
    }

    #[test]
    fn writing_up_an_idea_adds_to_its_waits() {
        let (conn, room, me) = setup();
        let x = task_in(&conn, &room, &me, Stage::Proposed);
        let y = task_in(&conn, &room, &me, Stage::Proposed);
        let idea = task_in(&conn, &room, &me, Stage::Idea);
        set_waits(&conn, &room, idea, &[x], NOW).unwrap();
        let task = propose_idea(&conn, &room, idea, "T", "", &[y, x], NOW).unwrap();
        assert_eq!(task.waits_for, vec![x, y]);
    }

    #[test]
    fn project_names_are_unique_case_insensitively() {
        let (conn, room, _) = setup();
        assert!(matches!(
            create_project(&conn, &room, "App", NOW),
            Err(Fail::Api(ApiError::ProjectExists))
        ));
        let site = create_project(&conn, &room, "site", NOW).unwrap();
        assert_eq!(site.id, 2);
        assert!(matches!(
            rename_project(&conn, &room, 2, "APP"),
            Err(Fail::Api(ApiError::ProjectExists))
        ));
        assert_eq!(rename_project(&conn, &room, 1, "App").unwrap().name, "App");
        assert!(matches!(
            rename_project(&conn, &room, 9, "x"),
            Err(Fail::Api(ApiError::NoSuchProject))
        ));
    }

    #[test]
    fn only_an_empty_project_is_deleted_and_its_id_is_not_given_again() {
        let (conn, room, me) = setup();
        assert_eq!(create_project(&conn, &room, "site", NOW).unwrap().id, 2);
        assert_eq!(create_project(&conn, &room, "old", NOW).unwrap().id, 3);
        let n = task_in(&conn, &room, &me, Stage::Idea);
        move_task(&conn, &room, n, 2, NOW).unwrap();

        assert!(matches!(
            delete_project(&conn, &room, 2),
            Err(Fail::Api(ApiError::ProjectNotEmpty))
        ));
        assert!(matches!(
            delete_project(&conn, &room, 9),
            Err(Fail::Api(ApiError::NoSuchProject))
        ));
        delete_project(&conn, &room, 3).unwrap();
        assert_eq!(
            read_projects(&conn, &room).unwrap(),
            vec![
                Project {
                    id: 1,
                    name: "app".into()
                },
                Project {
                    id: 2,
                    name: "site".into()
                }
            ]
        );
        assert!(matches!(
            delete_project(&conn, &room, 3),
            Err(Fail::Api(ApiError::NoSuchProject))
        ));
        assert_eq!(create_project(&conn, &room, "old", NOW).unwrap().id, 4);
    }

    #[test]
    fn claim_without_number_stays_in_its_project() {
        let (conn, room, me) = setup();
        create_project(&conn, &room, "site", NOW).unwrap();
        let first = task_in(&conn, &room, &me, Stage::Accepted);
        let second = task_in(&conn, &room, &me, Stage::Accepted);
        move_task(&conn, &room, second, 2, NOW).unwrap();

        let taken = claim(&conn, &room, &me, Work::Plan, Some(2), None, "a", NOW)
            .unwrap()
            .unwrap();
        assert_eq!(taken.number, second);
        assert!(
            claim(&conn, &room, &me, Work::Plan, Some(2), None, "a", NOW)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            claim(&conn, &room, &me, Work::Plan, Some(9), None, "a", NOW),
            Err(Fail::Api(ApiError::NoSuchProject))
        ));
        let rest = claim(&conn, &room, &me, Work::Plan, Some(1), None, "a", NOW)
            .unwrap()
            .unwrap();
        assert_eq!(rest.number, first);
    }

    #[test]
    fn move_task_needs_an_existing_project() {
        let (conn, room, me) = setup();
        create_project(&conn, &room, "site", NOW).unwrap();
        let n = task_in(&conn, &room, &me, Stage::Proposed);
        let moved = move_task(&conn, &room, n, 2, NOW + 5).unwrap();
        assert_eq!((moved.project, moved.updated_ms), (2, NOW + 5));
        assert!(matches!(
            move_task(&conn, &room, n, 9, NOW),
            Err(Fail::Api(ApiError::NoSuchProject))
        ));

        let done = task_in(&conn, &room, &me, Stage::Ready);
        claim(&conn, &room, &me, Work::Build, None, Some(done), "a", NOW).unwrap();
        finish(&conn, &room, &me, done, "", NOW).unwrap();
        assert_eq!(move_task(&conn, &room, done, 2, NOW).unwrap().project, 2);
    }
}
