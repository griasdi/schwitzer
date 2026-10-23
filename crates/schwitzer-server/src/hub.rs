//! In-memory registry: who listens for events, the live sessions members
//! published, and the running debug sessions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use schwitzer_proto::{
    ApiError, DebugId, DebugInstruction, DebugSession, Event, HEADER_LEN, MAX_EVENT_FRAME,
    MemberId, RoomId, SessionInfo, caps, encode_frame,
};
use tokio::sync::mpsc;

/// Frames queued per subscription. A full queue drops the subscription: its
/// stream ends, and the client reconnects and takes a new snapshot.
const QUEUE: usize = 1024;

/// Streams one member may hold; the oldest goes when it opens another.
const MAX_SUBS_PER_MEMBER: usize = 8;

/// An encoded, length-prefixed event frame, shared by every receiver.
pub type Frame = Arc<[u8]>;

/// Encodes `event` as one frame; `None` if it cannot be sent (and is logged).
pub fn frame_of(event: &Event) -> Option<Frame> {
    match encode_frame(event) {
        Ok(bytes) if bytes.len() - HEADER_LEN <= MAX_EVENT_FRAME => Some(bytes.into()),
        Ok(bytes) => {
            tracing::error!("event frame of {} bytes exceeds the limit", bytes.len());
            None
        }
        Err(e) => {
            tracing::error!("encoding an event failed: {e}");
            None
        }
    }
}

struct Sub {
    id: u64,
    tx: mpsc::Sender<Frame>,
}

/// Queues `frame` on `sub`; `false` if the stream is to be dropped (queue full or closed).
fn deliver(sub: &Sub, frame: &Frame) -> bool {
    match sub.tx.try_send(Arc::clone(frame)) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::warn!("event queue full; dropping a subscription");
            false
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

#[derive(Default)]
struct Inner {
    next: u64,
    subs: HashMap<MemberId, Vec<Sub>>,
}

/// A registered event stream.
pub struct Subscription {
    /// Id to pass to [`Hub::unsubscribe`].
    pub id: u64,
    /// Frames for this stream; ends when the hub drops the subscription.
    pub rx: mpsc::Receiver<Frame>,
    /// The member had no stream before this one.
    pub came_online: bool,
}

/// Registry of event subscriptions.
#[derive(Default)]
pub struct Hub {
    inner: Mutex<Inner>,
}

impl Hub {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn subscribe(&self, member: MemberId) -> Subscription {
        let (tx, rx) = mpsc::channel(QUEUE);
        let mut inner = self.lock();
        let id = inner.next;
        inner.next += 1;
        let subs = inner.subs.entry(member).or_default();
        let came_online = subs.is_empty();
        if subs.len() >= MAX_SUBS_PER_MEMBER {
            subs.remove(0);
        }
        subs.push(Sub { id, tx });
        Subscription {
            id,
            rx,
            came_online,
        }
    }

    /// Removes a subscription; `true` if the member has no stream left. A
    /// subscription the hub already dropped is a no-op, and its owner still
    /// learns whether the member is gone.
    pub fn unsubscribe(&self, member: &MemberId, id: u64) -> bool {
        let mut inner = self.lock();
        if let Some(subs) = inner.subs.get_mut(member) {
            subs.retain(|s| s.id != id);
            if subs.is_empty() {
                inner.subs.remove(member);
            }
        }
        !inner.subs.contains_key(member)
    }

    pub fn is_online(&self, member: &MemberId) -> bool {
        self.lock().subs.contains_key(member)
    }

    /// Sends `event` to every stream of `members`. A stream whose queue is full
    /// or closed is dropped.
    pub fn publish(&self, members: &[MemberId], event: &Event) {
        let Some(frame) = frame_of(event) else {
            return;
        };
        let mut inner = self.lock();
        for member in members {
            let Some(subs) = inner.subs.get_mut(member) else {
                continue;
            };
            subs.retain(|s| deliver(s, &frame));
            if subs.is_empty() {
                inner.subs.remove(member);
            }
        }
    }

    /// Sends `event` to every stream of every member.
    pub fn publish_all(&self, event: &Event) {
        let Some(frame) = frame_of(event) else {
            return;
        };
        self.lock().subs.retain(|_, subs| {
            subs.retain(|s| deliver(s, &frame));
            !subs.is_empty()
        });
    }
}

/// Sessions members published, held in memory only.
#[derive(Default)]
pub struct Sessions {
    map: Mutex<HashMap<(RoomId, MemberId), Vec<SessionInfo>>>,
}

impl Sessions {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(RoomId, MemberId), Vec<SessionInfo>>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces the list of `member` in `room`; an empty list removes it.
    pub fn set(&self, room: RoomId, member: MemberId, sessions: Vec<SessionInfo>) {
        let mut map = self.lock();
        if sessions.is_empty() {
            map.remove(&(room, member));
        } else {
            map.insert((room, member), sessions);
        }
    }

    pub fn clear(&self, room: &RoomId, member: &MemberId) {
        self.lock().remove(&(*room, *member));
    }

    /// Drops every list of `member`.
    pub fn clear_member(&self, member: &MemberId) {
        self.lock().retain(|(_, m), _| m != member);
    }

    /// The non-empty lists of `room`, ordered by member.
    pub fn of_room(&self, room: &RoomId) -> Vec<(MemberId, Vec<SessionInfo>)> {
        let map = self.lock();
        let mut lists: Vec<_> = map
            .iter()
            .filter(|((r, _), _)| r == room)
            .map(|((_, m), s)| (*m, s.clone()))
            .collect();
        lists.sort_by_key(|(m, _)| *m);
        lists
    }
}

struct DebugRun {
    session: DebugSession,
    next_instruction: u64,
}

/// The running debug sessions, at most one per room, held in memory only.
#[derive(Default)]
pub struct DebugRuns {
    map: Mutex<HashMap<RoomId, DebugRun>>,
}

impl DebugRuns {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<RoomId, DebugRun>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn of_room(&self, room: &RoomId) -> Option<DebugSession> {
        self.lock().get(room).map(|run| run.session.clone())
    }

    /// Starts a session in `room` with `master` as its master and first member.
    pub fn start(
        &self,
        room: RoomId,
        master: MemberId,
        now_ms: u64,
    ) -> Result<DebugSession, ApiError> {
        let mut map = self.lock();
        if map.contains_key(&room) {
            return Err(ApiError::DebugRunning);
        }
        let session = DebugSession {
            id: DebugId::random(),
            master,
            members: vec![master],
            started_ms: now_ms,
        };
        map.insert(
            room,
            DebugRun {
                session: session.clone(),
                next_instruction: 1,
            },
        );
        Ok(session)
    }

    /// Adds `member` to the session; `true` when that changed it.
    pub fn join(
        &self,
        room: &RoomId,
        session: DebugId,
        member: MemberId,
    ) -> Result<(DebugSession, bool), ApiError> {
        let mut map = self.lock();
        let run = running(&mut map, room, session)?;
        if run.session.members.contains(&member) {
            return Ok((run.session.clone(), false));
        }
        if run.session.members.len() >= caps::DEBUG_MEMBERS {
            return Err(ApiError::Invalid(format!(
                "the debug session has {} members already",
                caps::DEBUG_MEMBERS
            )));
        }
        run.session.members.push(member);
        Ok((run.session.clone(), true))
    }

    /// `member` leaves the session; the master leaving, or the last member, ends it
    /// (`Ok(None)`).
    pub fn leave(
        &self,
        room: &RoomId,
        session: DebugId,
        member: MemberId,
    ) -> Result<Option<DebugSession>, ApiError> {
        let mut map = self.lock();
        let run = running(&mut map, room, session)?;
        if run.session.master != member && !run.session.members.contains(&member) {
            return Err(ApiError::NotInDebugSession);
        }
        if run.session.master == member {
            map.remove(room);
            return Ok(None);
        }
        run.session.members.retain(|m| *m != member);
        if run.session.members.is_empty() {
            map.remove(room);
            return Ok(None);
        }
        Ok(Some(run.session.clone()))
    }

    /// Drops `member` from the members of `room`'s session, because they left the room or
    /// disconnected; the master stays master. `None`: nothing changed; `Some(None)`: the
    /// session ended because nobody is left.
    pub fn remove_member(&self, room: &RoomId, member: &MemberId) -> Option<Option<DebugSession>> {
        let mut map = self.lock();
        remove_from(&mut map, room, member)
    }

    /// [`Self::remove_member`] for every room; the rooms whose session changed.
    pub fn remove_member_everywhere(
        &self,
        member: &MemberId,
    ) -> Vec<(RoomId, Option<DebugSession>)> {
        let mut map = self.lock();
        let rooms: Vec<RoomId> = map.keys().copied().collect();
        rooms
            .into_iter()
            .filter_map(|room| remove_from(&mut map, &room, member).map(|s| (room, s)))
            .collect()
    }

    /// The members who receive `from`'s log bytes: every other member.
    pub fn log_targets(
        &self,
        room: &RoomId,
        session: DebugId,
        from: &MemberId,
    ) -> Result<Vec<MemberId>, ApiError> {
        let mut map = self.lock();
        let run = running(&mut map, room, session)?;
        if !run.session.members.contains(from) {
            return Err(ApiError::NotInDebugSession);
        }
        Ok(run
            .session
            .members
            .iter()
            .filter(|m| *m != from)
            .copied()
            .collect())
    }

    /// The master's instruction to `to` (empty: every participant); the instruction and
    /// whom to publish it to: the recipients and the master.
    pub fn instruct(
        &self,
        room: &RoomId,
        session: DebugId,
        from: MemberId,
        to: Vec<MemberId>,
        text: String,
        now_ms: u64,
    ) -> Result<(DebugInstruction, Vec<MemberId>), ApiError> {
        let mut map = self.lock();
        let run = running(&mut map, room, session)?;
        let s = &run.session;
        if from != s.master {
            return Err(ApiError::NotDebugMaster);
        }
        if !s.members.contains(&from) {
            return Err(ApiError::NotInDebugSession);
        }
        if text.trim().is_empty() {
            return Err(ApiError::Invalid("the instruction is empty".into()));
        }
        if text.len() > caps::INSTRUCTION {
            return Err(ApiError::Invalid(format!(
                "the instruction is longer than {} bytes",
                caps::INSTRUCTION
            )));
        }
        let to = if to.is_empty() {
            s.members
                .iter()
                .filter(|m| **m != s.master)
                .copied()
                .collect()
        } else {
            let mut unique: Vec<MemberId> = Vec::with_capacity(to.len());
            for m in to {
                if m == s.master || !s.members.contains(&m) {
                    return Err(ApiError::Invalid(format!(
                        "{} has not joined the debug session",
                        m.short()
                    )));
                }
                if !unique.contains(&m) {
                    unique.push(m);
                }
            }
            unique
        };
        if to.is_empty() {
            return Err(ApiError::Invalid(
                "nobody else has joined the debug session".into(),
            ));
        }
        let id = run.next_instruction;
        run.next_instruction += 1;
        let mut publish = to.clone();
        publish.push(from);
        Ok((
            DebugInstruction {
                id,
                from,
                to,
                text,
                sent_ms: now_ms,
            },
            publish,
        ))
    }
}

/// The run of `room` if it is `session`.
fn running<'a>(
    map: &'a mut HashMap<RoomId, DebugRun>,
    room: &RoomId,
    session: DebugId,
) -> Result<&'a mut DebugRun, ApiError> {
    map.get_mut(room)
        .filter(|run| run.session.id == session)
        .ok_or(ApiError::NoDebugSession)
}

fn remove_from(
    map: &mut HashMap<RoomId, DebugRun>,
    room: &RoomId,
    member: &MemberId,
) -> Option<Option<DebugSession>> {
    let run = map.get_mut(room)?;
    if !run.session.members.contains(member) {
        return None;
    }
    run.session.members.retain(|m| m != member);
    if run.session.members.is_empty() {
        map.remove(room);
        return Some(None);
    }
    Some(Some(run.session.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(n: u8) -> MemberId {
        MemberId([n; 32])
    }

    #[test]
    fn presence_follows_the_last_stream() {
        let hub = Hub::default();
        let a = hub.subscribe(member(1));
        let b = hub.subscribe(member(1));
        assert!(a.came_online);
        assert!(!b.came_online);
        assert!(hub.is_online(&member(1)));
        assert!(!hub.unsubscribe(&member(1), a.id));
        assert!(hub.unsubscribe(&member(1), b.id));
        assert!(!hub.is_online(&member(1)));
    }

    #[test]
    fn a_full_queue_drops_the_subscription() {
        let hub = Hub::default();
        let mut sub = hub.subscribe(member(1));
        for _ in 0..=QUEUE {
            hub.publish(&[member(1)], &Event::Ready);
        }
        assert!(!hub.is_online(&member(1)));
        let mut received = 0;
        while sub.rx.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(received, QUEUE);
        assert!(hub.unsubscribe(&member(1), sub.id));
    }

    #[test]
    fn a_debug_session_runs_until_its_master_leaves() {
        let runs = DebugRuns::default();
        let room = RoomId([9; 32]);
        let s = runs.start(room, member(1), 0).unwrap();
        assert_eq!(
            runs.start(room, member(2), 0).unwrap_err(),
            ApiError::DebugRunning
        );
        assert_eq!(
            runs.join(&room, DebugId([0; 16]), member(2)).unwrap_err(),
            ApiError::NoDebugSession
        );
        assert!(runs.join(&room, s.id, member(2)).unwrap().1);
        assert!(!runs.join(&room, s.id, member(2)).unwrap().1);
        runs.join(&room, s.id, member(3)).unwrap();

        let instruct =
            |from, to: Vec<MemberId>| runs.instruct(&room, s.id, from, to, "run it".into(), 0);
        assert_eq!(
            instruct(member(2), vec![]).unwrap_err(),
            ApiError::NotDebugMaster
        );
        assert!(matches!(
            instruct(member(1), vec![member(1)]),
            Err(ApiError::Invalid(_))
        ));
        assert!(matches!(
            instruct(member(1), vec![member(4)]),
            Err(ApiError::Invalid(_))
        ));
        let (first, publish) = instruct(member(1), vec![]).unwrap();
        assert_eq!(first.id, 1);
        assert_eq!(first.to, vec![member(2), member(3)]);
        assert_eq!(publish, vec![member(2), member(3), member(1)]);
        let (second, _) = instruct(member(1), vec![member(3)]).unwrap();
        assert_eq!(second.id, 2);
        assert_eq!(second.to, vec![member(3)]);

        assert_eq!(
            runs.leave(&room, s.id, member(2)).unwrap().unwrap().members,
            vec![member(1), member(3)]
        );
        assert_eq!(runs.leave(&room, s.id, member(1)).unwrap(), None);
        assert_eq!(runs.of_room(&room), None);
    }

    #[test]
    fn a_disconnected_master_stays_master_until_nobody_is_left() {
        let runs = DebugRuns::default();
        let room = RoomId([9; 32]);
        let s = runs.start(room, member(1), 0).unwrap();
        runs.join(&room, s.id, member(2)).unwrap();
        let after = runs.remove_member(&room, &member(1)).unwrap().unwrap();
        assert_eq!(after.master, member(1));
        assert_eq!(after.members, vec![member(2)]);
        assert_eq!(runs.remove_member(&room, &member(1)), None);
        assert_eq!(
            runs.remove_member_everywhere(&member(2)),
            vec![(room, None)]
        );
        assert_eq!(runs.of_room(&room), None);
    }
}
