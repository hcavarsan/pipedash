use std::fmt;

use tokio::time::Instant;

use super::{
    AttemptHandle,
    FetchClaim,
    FetchResult,
    ForcedWait,
    Generation,
    Outcome,
    ScheduleView,
};

impl Drop for ForcedWait {
    fn drop(&mut self) {
        if let Some(entry) = self
            .scheduler
            .entries()
            .get_mut(&self.id)
            .filter(|entry| entry.generation == self.generation)
        {
            entry.forced_waiters = entry.forced_waiters.saturating_sub(1);
        }
    }
}

impl fmt::Debug for FetchClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FetchClaim")
            .field("id", &self.id)
            .field("generation", &self.generation)
            .finish()
    }
}

impl FetchClaim {
    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn handle(&self) -> AttemptHandle {
        AttemptHandle(self.result.subscribe())
    }

    pub fn complete(
        mut self, now: Instant, outcome: Outcome, result: FetchResult,
    ) -> Option<ScheduleView> {
        self.done = true;
        let view = self
            .scheduler
            .finish(self.id, self.generation, now, outcome);
        self.result.send_replace(Some(result));
        view
    }

    pub fn release(mut self) {
        self.done = true;
        self.scheduler
            .release(self.id, self.generation, self.previous_attempt);
        self.result.send_replace(Some(FetchResult::Cancelled));
    }
}

impl Drop for FetchClaim {
    fn drop(&mut self) {
        if !self.done {
            self.scheduler
                .finish(self.id, self.generation, Instant::now(), Outcome::Cancelled);
        }
    }
}

impl fmt::Debug for AttemptHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AttemptHandle")
    }
}

impl AttemptHandle {
    pub async fn wait(mut self) -> FetchResult {
        match self.0.wait_for(Option::is_some).await {
            Ok(result) => result.clone().unwrap_or(FetchResult::Cancelled),
            Err(_) => FetchResult::Cancelled,
        }
    }
}
