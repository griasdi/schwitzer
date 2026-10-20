//! Per-member request budget, in memory: a member may send `BURST` requests at once and
//! regains one per `INTERVAL`.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use schwitzer_proto::MemberId;

/// A member regains one request per interval: 60 a minute.
const INTERVAL: Duration = Duration::from_secs(1);
/// Requests a member may send at once. A reconnect publishes the sessions of every room
/// (up to `MAX_ROOMS_PER_MEMBER` = 50) in one go, so the burst covers that.
const BURST: u32 = 60;
/// Members tracked before those whose budget is full again are forgotten.
const PRUNE_AT: usize = 4096;

/// Per member, the time at which their budget is full again (GCRA's theoretical arrival time).
#[derive(Default)]
pub struct Limits {
    full_at: Mutex<HashMap<MemberId, Instant>>,
}

impl Limits {
    /// Spends one request of `member`'s budget at `now`; `Err(wait)` when it is used up,
    /// with the time until the next request is allowed.
    pub fn take(&self, member: MemberId, now: Instant) -> Result<(), Duration> {
        let mut full_at = self.full_at.lock().unwrap_or_else(PoisonError::into_inner);
        if full_at.len() >= PRUNE_AT {
            full_at.retain(|_, at| *at > now);
        }
        let at = full_at
            .get(&member)
            .copied()
            .filter(|at| *at > now)
            .unwrap_or(now);
        let ahead = at - now;
        let allowed = INTERVAL * (BURST - 1);
        if ahead > allowed {
            return Err(ahead - allowed);
        }
        full_at.insert(member, at + INTERVAL);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn use_up(limits: &Limits, member: MemberId, now: Instant) {
        for _ in 0..BURST {
            assert_eq!(limits.take(member, now), Ok(()));
        }
        assert!(limits.take(member, now).is_err());
    }

    #[test]
    fn a_burst_then_one_per_interval() {
        let limits = Limits::default();
        let t0 = Instant::now();
        let m = MemberId([1; 32]);
        for _ in 0..60 {
            assert_eq!(limits.take(m, t0), Ok(()));
        }
        assert_eq!(limits.take(m, t0), Err(Duration::from_secs(1)));
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(limits.take(m, t1), Ok(()));
        assert_eq!(limits.take(m, t1), Err(Duration::from_secs(1)));
    }

    #[test]
    fn members_have_their_own_budget() {
        let limits = Limits::default();
        let t0 = Instant::now();
        use_up(&limits, MemberId([1; 32]), t0);
        assert_eq!(limits.take(MemberId([2; 32]), t0), Ok(()));
    }

    #[test]
    fn an_idle_member_gets_the_whole_burst_back() {
        let limits = Limits::default();
        let t0 = Instant::now();
        let m = MemberId([1; 32]);
        use_up(&limits, m, t0);
        use_up(&limits, m, t0 + Duration::from_secs(60));
    }

    #[test]
    fn full_budgets_are_forgotten() {
        let limits = Limits::default();
        let t0 = Instant::now();
        for n in 0..PRUNE_AT {
            let mut id = [0xff; 32];
            id[..2].copy_from_slice(&u16::try_from(n).unwrap().to_le_bytes());
            assert_eq!(limits.take(MemberId(id), t0), Ok(()));
        }
        assert_eq!(
            limits.take(MemberId([0; 32]), t0 + Duration::from_secs(61)),
            Ok(())
        );
        assert_eq!(limits.full_at.lock().unwrap().len(), 1);
    }
}
