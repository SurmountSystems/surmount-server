//! In-memory fixed-window rate limiter for the Axum edge.
//!
//! Skeleton for merciless access control: edge rate limits first; host
//! ban/whitelist path lives in networking/hardening modules (nft lean).
//! Not a full product ban system yet.
//!
//! Dual-run note: when nginx proxies to loopback, key clients via
//! `client_ip_for_rate_limit` (X-Real-IP only from loopback peers; no XFF).

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Default cap on distinct keys retained in memory.
pub const DEFAULT_MAX_KEYS: usize = 50_000;

/// Fixed-window counter: at most `max_requests` per `window` per key.
#[derive(Debug)]
pub struct FixedWindowRateLimiter<K> {
    max_requests: u32,
    window: Duration,
    max_keys: usize,
    state: Mutex<HashMap<K, WindowEntry>>,
}

#[derive(Debug, Clone)]
struct WindowEntry {
    window_start: Instant,
    count: u32,
}

/// Outcome of a rate-limit check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allowed { remaining: u32 },
    Limited { retry_after: Duration },
}

impl<K: Eq + Hash + Clone> FixedWindowRateLimiter<K> {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self::with_max_keys(max_requests, window, DEFAULT_MAX_KEYS)
    }

    pub fn with_max_keys(max_requests: u32, window: Duration, max_keys: usize) -> Self {
        assert!(max_requests > 0, "max_requests must be > 0");
        assert!(!window.is_zero(), "window must be non-zero");
        assert!(max_keys > 0, "max_keys must be > 0");
        Self {
            max_requests,
            window,
            max_keys,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Record one request for `key` at `now`. Returns allow or limit.
    pub fn check(&self, key: K, now: Instant) -> RateLimitDecision {
        let mut map = self.state.lock().expect("rate limiter mutex");

        // Drop expired windows first so capacity can recover.
        map.retain(|_, entry| now.duration_since(entry.window_start) < self.window);

        let is_new = !map.contains_key(&key);
        if is_new && map.len() >= self.max_keys {
            // At capacity: refuse new keys rather than grow forever.
            return RateLimitDecision::Limited {
                retry_after: self.window,
            };
        }

        let entry = map.entry(key).or_insert(WindowEntry {
            window_start: now,
            count: 0,
        });

        if now.duration_since(entry.window_start) >= self.window {
            entry.window_start = now;
            entry.count = 0;
        }

        if entry.count >= self.max_requests {
            let elapsed = now.duration_since(entry.window_start);
            let retry_after = self.window.saturating_sub(elapsed);
            return RateLimitDecision::Limited { retry_after };
        }

        entry.count += 1;
        let remaining = self.max_requests.saturating_sub(entry.count);
        RateLimitDecision::Allowed { remaining }
    }

    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    pub fn max_keys(&self) -> usize {
        self.max_keys
    }

    /// Test helper: number of tracked keys.
    #[cfg(test)]
    pub fn len_keys(&self) -> usize {
        self.state.lock().expect("rate limiter mutex").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_max_requests_in_window() {
        let lim = FixedWindowRateLimiter::new(3, Duration::from_secs(60));
        let t0 = Instant::now();
        assert!(matches!(
            lim.check("10.0.0.1", t0),
            RateLimitDecision::Allowed { remaining: 2 }
        ));
        assert!(matches!(
            lim.check("10.0.0.1", t0 + Duration::from_millis(1)),
            RateLimitDecision::Allowed { remaining: 1 }
        ));
        assert!(matches!(
            lim.check("10.0.0.1", t0 + Duration::from_millis(2)),
            RateLimitDecision::Allowed { remaining: 0 }
        ));
        assert!(matches!(
            lim.check("10.0.0.1", t0 + Duration::from_millis(3)),
            RateLimitDecision::Limited { .. }
        ));
    }

    #[test]
    fn separate_keys_have_independent_windows() {
        let lim = FixedWindowRateLimiter::new(1, Duration::from_secs(60));
        let t0 = Instant::now();
        assert!(matches!(
            lim.check("a", t0),
            RateLimitDecision::Allowed { .. }
        ));
        assert!(matches!(
            lim.check("b", t0),
            RateLimitDecision::Allowed { .. }
        ));
        assert!(matches!(
            lim.check("a", t0),
            RateLimitDecision::Limited { .. }
        ));
    }

    #[test]
    fn window_reset_allows_again() {
        let lim = FixedWindowRateLimiter::new(1, Duration::from_secs(10));
        let t0 = Instant::now();
        assert!(matches!(
            lim.check("k", t0),
            RateLimitDecision::Allowed { .. }
        ));
        assert!(matches!(
            lim.check("k", t0 + Duration::from_secs(1)),
            RateLimitDecision::Limited { .. }
        ));
        assert!(matches!(
            lim.check("k", t0 + Duration::from_secs(10)),
            RateLimitDecision::Allowed { remaining: 0 }
        ));
    }

    #[test]
    fn max_keys_cap_rejects_new_key_when_full() {
        let lim = FixedWindowRateLimiter::with_max_keys(5, Duration::from_secs(60), 2);
        let t0 = Instant::now();
        assert!(matches!(
            lim.check("a", t0),
            RateLimitDecision::Allowed { .. }
        ));
        assert!(matches!(
            lim.check("b", t0),
            RateLimitDecision::Allowed { .. }
        ));
        assert!(matches!(
            lim.check("c", t0),
            RateLimitDecision::Limited { .. }
        ));
        // Existing key still works within its budget.
        assert!(matches!(
            lim.check("a", t0),
            RateLimitDecision::Allowed { .. }
        ));
        assert_eq!(lim.len_keys(), 2);
    }
}
