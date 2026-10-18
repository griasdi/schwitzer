//! In-memory registry: who listens for events, and the live sessions members
//! published.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use schwitzer_proto::{Event, MAX_EVENT_FRAME, MemberId, RoomId, SessionInfo, encode_frame};
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
        Ok(bytes) if bytes.len() - 4 <= MAX_EVENT_FRAME => Some(bytes.into()),
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
}
