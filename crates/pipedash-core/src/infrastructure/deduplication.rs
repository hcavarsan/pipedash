use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{
    Hash,
    Hasher,
};
use std::sync::Arc;

use dashmap::DashMap;
use sha2::{
    Digest,
    Sha256,
};
use tokio::sync::oneshot;

use crate::application::Generation;
use crate::domain::PipelineRun;

type RequestId = u64;
type Waiters<T, E> = Vec<oneshot::Sender<Result<Arc<T>, E>>>;
type InFlight<T, E> = DashMap<RequestId, Waiters<T, E>>;

pub struct RequestDeduplicator<T: Clone, E: Clone> {
    in_flight: Arc<InFlight<T, E>>,
}

struct InFlightGuard<'a, T, E> {
    in_flight: &'a InFlight<T, E>,
    request_id: RequestId,
    armed: bool,
}

impl<T, E> InFlightGuard<'_, T, E> {
    fn take_waiters(mut self) -> Waiters<T, E> {
        self.armed = false;
        self.in_flight
            .remove(&self.request_id)
            .map(|(_, waiters)| waiters)
            .unwrap_or_default()
    }
}

impl<T, E> Drop for InFlightGuard<'_, T, E> {
    fn drop(&mut self) {
        if self.armed {
            self.in_flight.remove(&self.request_id);
        }
    }
}

impl<T: Clone, E: Clone> RequestDeduplicator<T, E> {
    pub fn new() -> Self {
        Self {
            in_flight: Arc::new(DashMap::new()),
        }
    }

    pub async fn deduplicate<F, Fut>(&self, request_id: RequestId, operation: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let waiter = match self.in_flight.entry(request_id) {
            dashmap::Entry::Occupied(mut entry) => {
                let (tx, rx) = oneshot::channel();
                entry.get_mut().push(tx);
                Some(rx)
            }
            dashmap::Entry::Vacant(entry) => {
                entry.insert(Vec::new());
                None
            }
        };

        if let Some(rx) = waiter {
            return match rx.await {
                Ok(shared) => shared.map(|value| (*value).clone()),
                Err(_) => operation().await,
            };
        }

        let guard = InFlightGuard {
            in_flight: &self.in_flight,
            request_id,
            armed: true,
        };
        let result = operation().await;
        let waiters = guard.take_waiters();

        if !waiters.is_empty() {
            let shared = match &result {
                Ok(value) => Ok(Arc::new(value.clone())),
                Err(e) => Err(e.clone()),
            };
            for waiter in waiters {
                let _ = waiter.send(shared.clone());
            }
        }

        result
    }
}

impl<T: Clone, E: Clone> Default for RequestDeduplicator<T, E> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn hash_request(provider_id: i64, generation: Generation, endpoint: &str) -> RequestId {
    let mut hasher = DefaultHasher::new();
    provider_id.hash(&mut hasher);
    generation.hash(&mut hasher);
    endpoint.hash(&mut hasher);
    hasher.finish()
}

pub fn hash_pipeline_run(run: &PipelineRun) -> String {
    let mut hasher = Sha256::new();
    hasher.update(run.run_number.to_string().as_bytes());
    hasher.update(run.status.as_str().as_bytes());
    hasher.update(run.branch.as_deref().unwrap_or("").as_bytes());
    hasher.update(run.started_at.to_rfc3339().as_bytes());
    if let Some(duration) = run.duration_seconds {
        hasher.update(duration.to_string().as_bytes());
    }
    hasher.update(run.commit_sha.as_deref().unwrap_or("").as_bytes());

    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl<T: Clone, E: Clone> Clone for RequestDeduplicator<T, E> {
    fn clone(&self) -> Self {
        Self {
            in_flight: self.in_flight.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
