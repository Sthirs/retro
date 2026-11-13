//! In-memory fixed-window limiter for failed sign-in attempts and other
//! abuse-prone actions (spec §20).
//! Per replica, which is acceptable at the target scale (spec §11.3).

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct RateLimiter {
    limit: u32,
    window: Duration,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// True when `key` has used up its failures for the current window.
    pub fn is_blocked(&self, key: &str) -> bool {
        let hits = self.hits.lock().expect("rate limiter poisoned");
        hits.get(key)
            .is_some_and(|(start, n)| start.elapsed() < self.window && *n >= self.limit)
    }

    /// Records one failure for `key`.
    pub fn record_failure(&self, key: &str) {
        let mut hits = self.hits.lock().expect("rate limiter poisoned");
        if hits.len() > 100_000 {
            let window = self.window;
            hits.retain(|_, (start, _)| start.elapsed() < window);
        }
        let entry = hits.entry(key.to_owned()).or_insert((Instant::now(), 0));
        if entry.0.elapsed() >= self.window {
            *entry = (Instant::now(), 0);
        }
        entry.1 += 1;
    }

    /// Forgets `key`, e.g. after a successful sign-in.
    pub fn reset(&self, key: &str) {
        self.hits.lock().expect("rate limiter poisoned").remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_limit_and_resets() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(!limiter.is_blocked("a"));
        limiter.record_failure("a");
        assert!(!limiter.is_blocked("a"));
        limiter.record_failure("a");
        assert!(limiter.is_blocked("a"));
        assert!(!limiter.is_blocked("b"));
        limiter.reset("a");
        assert!(!limiter.is_blocked("a"));
    }

    #[test]
    fn window_expires() {
        let limiter = RateLimiter::new(1, Duration::from_millis(1));
        limiter.record_failure("a");
        std::thread::sleep(Duration::from_millis(5));
        assert!(!limiter.is_blocked("a"));
    }
}
