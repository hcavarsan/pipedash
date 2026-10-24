use std::collections::HashMap;
use std::sync::{
    Mutex,
    PoisonError,
};
use std::time::{
    Duration,
    Instant,
};

struct Window {
    started: Instant,
    count: u32,
}

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

        match windows.get_mut(key) {
            Some(window) if now.duration_since(window.started) < self.window => {
                if window.count >= self.limit {
                    return false;
                }
                window.count += 1;
            }
            Some(window) => {
                *window = Window {
                    started: now,
                    count: 1,
                };
            }
            None => {
                windows.insert(
                    key.to_owned(),
                    Window {
                        started: now,
                        count: 1,
                    },
                );
            }
        }

        true
    }

    pub fn prune(&self, now: Instant) {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, window| now.duration_since(window.started) < self.window);
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
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
    fn pruning_forgets_only_the_keys_whose_window_has_ended() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        let start = Instant::now();
        assert!(limiter.allow("old", start));
        assert!(limiter.allow("recent", start + Duration::from_secs(50)));
        assert_eq!(limiter.tracked(), 2);

        limiter.prune(start + Duration::from_secs(59));
        assert_eq!(limiter.tracked(), 2);

        limiter.prune(start + Duration::from_secs(60));
        assert_eq!(limiter.tracked(), 1);
        assert!(!limiter.allow("recent", start + Duration::from_secs(61)));
        assert!(limiter.allow("old", start + Duration::from_secs(61)));

        limiter.prune(start + Duration::from_secs(200));
        assert_eq!(limiter.tracked(), 0);
    }
}
