//! The debug session lifecycle in the app: start, join and leave, the workers of the joined
//! sessions with the counts they report, and the notices a session change or an instruction
//! brings. The page itself is `debug_view.rs`, the worker `debug.rs`.

use iced::{Subscription, Task};
use schwitzer_client::InboxEntry;
use schwitzer_proto::{DebugId, RoomId};

use super::{App, Message, logic};
use crate::bridge::fire;
use crate::debug;

impl App {
    /// Starts a debug session in the selected room; this member becomes its master.
    pub(super) fn debug_start(&self) -> Task<Message> {
        let Some(room) = self.selected else {
            return Task::none();
        };
        let client = self.client.clone();
        fire(async move { client.debug_start(room).await })
    }

    /// Joins the selected room's debug session.
    pub(super) fn debug_join(&self) -> Task<Message> {
        let Some(room) = self.selected else {
            return Task::none();
        };
        let client = self.client.clone();
        fire(async move { client.debug_join(room).await })
    }

    /// Leaves the selected room's debug session; the master ends it for everyone.
    pub(super) fn debug_leave(&mut self) -> Task<Message> {
        self.confirm = None;
        let Some(room) = self.selected else {
            return Task::none();
        };
        let client = self.client.clone();
        fire(async move { client.debug_leave(room).await })
    }

    /// Puts a text of the Debug page on the clipboard.
    pub(super) fn copy_debug(&mut self, text: String) -> Task<Message> {
        self.notify("Copied");
        iced::clipboard::write(text)
    }

    /// One worker per debug session this member has joined.
    pub(super) fn debug_subscriptions(&self) -> Vec<Subscription<Message>> {
        self.rooms
            .ids()
            .filter_map(|room| {
                let session = self.joined_debug(room)?;
                Some(
                    debug::subscription(self.client.clone(), self.data_dir.clone(), room, session)
                        .map(Message::DebugTraffic),
                )
            })
            .collect()
    }

    /// A worker's counts changed; counts of a session this member has left are dropped.
    pub(super) fn on_debug_traffic(&mut self, traffic: debug::Traffic) {
        if self.joined_debug(traffic.room) == Some(traffic.session) {
            self.debug_traffic.insert(traffic.room, traffic);
        }
    }

    /// The room's debug inbox changed: tells of an instruction that arrived for this member.
    pub(super) fn on_debug_inbox(&mut self, room: RoomId, fresh: Option<InboxEntry>) {
        let Some(entry) = fresh.filter(|e| e.for_me) else {
            return;
        };
        let members = self
            .rooms
            .state(room)
            .map_or(&[][..], |s| s.members.as_slice());
        let from = logic::member_name(members, &entry.instruction.from);
        self.notify(&format!(
            "Debug instruction #{} from {from}",
            entry.instruction.id
        ));
    }

    /// A room's state changed: tells `notice` (a session started or ended, see
    /// [`logic::debug_change`]) and forgets the counts of sessions that are over.
    pub(super) fn on_debug_room_change(&mut self, notice: Option<String>) {
        if let Some(text) = notice {
            self.notify(&text);
        }
        self.prune_debug_traffic();
    }

    /// The debug session of `room` that this member has joined.
    fn joined_debug(&self, room: RoomId) -> Option<DebugId> {
        let me = self.client.member();
        self.rooms
            .state(room)?
            .debug
            .as_ref()
            .filter(|session| session.members.contains(&me))
            .map(|session| session.id)
    }

    /// Forgets the counts of workers whose session is over, was left or was replaced.
    pub(super) fn prune_debug_traffic(&mut self) {
        let mut traffic = std::mem::take(&mut self.debug_traffic);
        traffic.retain(|room, t| self.joined_debug(*room) == Some(t.session));
        self.debug_traffic = traffic;
    }
}
