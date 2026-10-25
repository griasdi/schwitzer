//! Per-member request budget, in memory: a member may send `BURST` requests at once and
//! regains one per `INTERVAL`. Debug logs have their own budget in bytes, [`ByteBudget`].

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

/// Per member, the time at which their budget is full again (GCRA's theoretical arrival
/// time): spending `cost` moves it forward by `cost`, and a spend is refused when that
/// would put it more than `tolerance` ahead of now.
#[derive(Default)]
struct Gcra {
    full_at: Mutex<HashMap<MemberId, Instant>>,
}

impl Gcra {
    /// Spends `cost` of `member`'s budget at `now`; `Err(wait)` when that would overdraw it,
    /// with the time until it is allowed. A refused spend costs nothing.
    fn take(
        &self,
        member: MemberId,
        cost: Duration,
        tolerance: Duration,
        now: Instant,
    ) -> Result<(), Duration> {
        let mut full_at = self.full_at.lock().unwrap_or_else(PoisonError::into_inner);
        if full_at.len() >= PRUNE_AT {
            full_at.retain(|_, at| *at > now);
        }
        let at = full_at
            .get(&member)
            .copied()
            .filter(|at| *at > now)
            .unwrap_or(now);
        let new = at + cost;
        let ahead = new - now;
        if ahead > tolerance {
            return Err(ahead - tolerance);
        }
        full_at.insert(member, new);
        Ok(())
    }
}

/// Per-member request budget: `BURST` at once, one more per `INTERVAL`.
#[derive(Default)]
pub struct Limits {
    gcra: Gcra,
}

impl Limits {
    /// Spends one request of `member`'s budget at `now`; `Err(wait)` when it is used up,
    /// with the time until the next request is allowed.
    pub fn take(&self, member: MemberId, now: Instant) -> Result<(), Duration> {
        self.gcra.take(member, INTERVAL, INTERVAL * BURST, now)
    }
}

/// Debug log bytes a member regains per second.
const LOG_RATE: u64 = 256 * 1024;
/// Debug log bytes a member may send at once.
const LOG_BURST: u64 = 1024 * 1024;

/// Per-member budget of debug log bytes: `LOG_BURST` at once, `LOG_RATE` per second
/// (GCRA over bytes).
#[derive(Default)]
pub struct ByteBudget {
    gcra: Gcra,
}

impl ByteBudget {
    /// Spends `bytes` of `member`'s budget at `now`; `Err(wait)` when that would overdraw it,
    /// with the time until it is allowed. A refused request spends nothing.
    pub fn take(&self, member: MemberId, bytes: usize, now: Instant) -> Result<(), Duration> {
        let cost = Duration::from_nanos(bytes as u64 * 1_000_000_000 / LOG_RATE);
        let tolerance = Duration::from_nanos(LOG_BURST * 1_000_000_000 / LOG_RATE);
        self.gcra.take(member, cost, tolerance, now)
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
        assert_eq!(limits.gcra.full_at.lock().unwrap().len(), 1);
    }

    #[test]
    fn log_bytes_have_a_burst_then_a_rate() {
        let budget = ByteBudget::default();
        let t0 = Instant::now();
        let m = MemberId([1; 32]);
        assert_eq!(budget.take(m, 1024 * 1024, t0), Ok(()));
        assert_eq!(
            budget.take(m, 64 * 1024, t0),
            Err(Duration::from_millis(250))
        );
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(budget.take(m, 256 * 1024, t1), Ok(()));
        assert!(budget.take(m, 1, t1).is_err());
        assert_eq!(budget.take(MemberId([2; 32]), 1024 * 1024, t1), Ok(()));
    }
}
