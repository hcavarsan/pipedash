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
mod tests;
