//! The running debug sessions: who is in one, and the master's instructions.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use schwitzer_proto::{ApiError, DebugId, DebugInstruction, DebugSession, MemberId, RoomId, caps};

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
        Ok(remove_from(&mut map, room, &member).flatten())
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
