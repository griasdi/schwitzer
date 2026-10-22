//! The rooms the user keeps, as the window shows them: the client's list with each room's last
//! known state, looked up by room id.

use schwitzer_client::Client;
use schwitzer_proto::{RoomId, RoomKey, RoomState, ServerAddr};

struct Entry {
    id: RoomId,
    key: RoomKey,
    state: Option<RoomState>,
}

/// A copy of `Client::rooms()` in list order; `view` borrows the states from it, which the
/// client only hands out as clones.
#[derive(Default)]
pub struct Rooms(Vec<Entry>);

impl Rooms {
    pub fn new(client: &Client) -> Self {
        let mut rooms = Self::default();
        rooms.reload(client);
        rooms
    }

    /// Takes the list and every state from the client.
    pub fn reload(&mut self, client: &Client) {
        self.0 = client
            .rooms()
            .into_iter()
            .map(|(key, state)| Entry {
                id: RoomId::of(&key.token),
                key,
                state,
            })
            .collect();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The rooms in list order: id, key and last known state.
    pub fn iter(&self) -> impl Iterator<Item = (RoomId, &RoomKey, Option<&RoomState>)> {
        self.0
            .iter()
            .map(|entry| (entry.id, &entry.key, entry.state.as_ref()))
    }

    pub fn ids(&self) -> impl Iterator<Item = RoomId> {
        self.0.iter().map(|entry| entry.id)
    }

    /// The ids of the rooms on `server`.
    pub fn ids_on<'a>(&'a self, server: &'a ServerAddr) -> impl Iterator<Item = RoomId> + 'a {
        self.0
            .iter()
            .filter(move |entry| entry.key.server == *server)
            .map(|entry| entry.id)
    }

    pub fn contains(&self, id: RoomId) -> bool {
        self.entry(id).is_some()
    }

    pub fn key(&self, id: RoomId) -> Option<&RoomKey> {
        self.entry(id).map(|entry| &entry.key)
    }

    pub fn state(&self, id: RoomId) -> Option<&RoomState> {
        self.entry(id)?.state.as_ref()
    }

    /// Whether any room is on `server`.
    pub fn has_server(&self, server: &ServerAddr) -> bool {
        self.0.iter().any(|entry| entry.key.server == *server)
    }

    /// Stores the room's new state. `None`: the room is not in the list. `Some(old)`: the
    /// state it had, `None` while the room had none yet.
    pub fn set_state(&mut self, id: RoomId, state: RoomState) -> Option<Option<RoomState>> {
        let entry = self.0.iter_mut().find(|entry| entry.id == id)?;
        Some(entry.state.replace(state))
    }

    fn entry(&self, id: RoomId) -> Option<&Entry> {
        self.0.iter().find(|entry| entry.id == id)
    }
}
