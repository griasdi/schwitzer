//! The wait between two tries of something that keeps failing: it starts at
//! `first`, doubles with every failure and stops growing at `max`.

use std::time::Duration;

pub(crate) struct Backoff {
    first: Duration,
    max: Duration,
    cur: Duration,
}

impl Backoff {
    pub(crate) fn new(first: Duration, max: Duration) -> Self {
        Self {
            first,
            max,
            cur: first,
        }
    }

    /// The wait for this failure; the one after it is twice as long, up to `max`.
    pub(crate) fn next(&mut self) -> Duration {
        let wait = self.cur;
        self.cur = (self.cur * 2).min(self.max);
        wait
    }

    /// Starts over at `first`: what failed works again.
    pub(crate) fn reset(&mut self) {
        self.cur = self.first;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn the_wait_doubles_up_to_the_maximum_and_starts_over_on_reset() {
        let mut backoff = Backoff::new(secs(1), secs(10));
        let waits: Vec<_> = (0..6).map(|_| backoff.next()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 10, 10].map(secs));
        backoff.reset();
        assert_eq!(backoff.next(), secs(1));
    }
}
