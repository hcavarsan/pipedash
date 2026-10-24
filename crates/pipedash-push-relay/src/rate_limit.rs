use std::collections::HashMap;
use std::sync::{
    Mutex,
    PoisonError,
};
use std::time::{
    Duration,
    Instant,
};

const PRUNE_AT: usize = 10_000;

struct Window {
    started: Instant,
    count: u32,
}

// shortcut: fixed windows allow up to twice the limit across a window edge,
// move to a sliding window if that ever matters
pub struct RateLimiter {
    limit: u32,
    window: Duration,
    windows: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, key: &str, now: Instant) -> bool {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);

        if let Some(window) = windows.get_mut(key) {
            if now.duration_since(window.started) >= self.window {
                *window = Window {
                    started: now,
                    count: 1,
                };
                return true;
            }
            if window.count >= self.limit {
                return false;
            }
            window.count += 1;
            return true;
        }

        if windows.len() >= PRUNE_AT {
            windows.retain(|_, window| now.duration_since(window.started) < self.window);
        }
        windows.insert(
            key.to_owned(),
            Window {
                started: now,
                count: 1,
            },
        );

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_gets_its_limit_per_window_and_no_more() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        let start = Instant::now();

        assert!((0..3).all(|_| limiter.allow("a", start)));
        assert!(!limiter.allow("a", start));
        assert!(!limiter.allow("a", start + Duration::from_secs(59)));
    }

    #[test]
    fn the_window_resets_after_it_elapses() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        let start = Instant::now();

        assert!(limiter.allow("a", start));
        assert!(!limiter.allow("a", start));
        assert!(limiter.allow("a", start + Duration::from_secs(60)));
        assert!(!limiter.allow("a", start + Duration::from_secs(61)));
    }

    #[test]
    fn keys_are_limited_independently() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        let start = Instant::now();

        assert!(limiter.allow("a", start));
        assert!(limiter.allow("b", start));
        assert!(!limiter.allow("a", start));
        assert!(!limiter.allow("b", start));
    }

    #[test]
    fn expired_windows_are_dropped_once_the_map_is_full() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        let start = Instant::now();

        for i in 0..PRUNE_AT {
            assert!(limiter.allow(&i.to_string(), start));
        }
        assert!(limiter.allow("fresh", start + Duration::from_secs(120)));

        let windows = limiter.windows.lock().unwrap();
        assert_eq!(windows.len(), 1);
    }
}
