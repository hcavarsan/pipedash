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

pub fn hash_pipeline_run(
    run_number: i64, status: &str, branch: Option<&str>, started_at: &str,
    duration_seconds: Option<i64>, commit_sha: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(run_number.to_string().as_bytes());
    hasher.update(status.as_bytes());
    hasher.update(branch.unwrap_or("").as_bytes());
    hasher.update(started_at.as_bytes());
    if let Some(duration) = duration_seconds {
        hasher.update(duration.to_string().as_bytes());
    }
    hasher.update(commit_sha.unwrap_or("").as_bytes());

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
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use super::*;

    #[tokio::test]
    async fn test_request_deduplication() {
        let dedup = RequestDeduplicator::<i32, ()>::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let request_id = hash_request(1, Generation::next(), "test");

        let dedup1 = dedup.clone();
        let count1 = call_count.clone();
        let handle1 = tokio::spawn(async move {
            dedup1
                .deduplicate(request_id, || async {
                    count1.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    Ok::<_, ()>(42)
                })
                .await
        });

        let dedup2 = dedup.clone();
        let count2 = call_count.clone();
        let handle2 = tokio::spawn(async move {
            dedup2
                .deduplicate(request_id, || async {
                    count2.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    Ok::<_, ()>(42)
                })
                .await
        });

        let (r1, r2) = tokio::join!(handle1, handle2);
        assert_eq!(r1.unwrap().unwrap(), 42);
        assert_eq!(r2.unwrap().unwrap(), 42);

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_request_does_not_block_the_next_one() {
        let dedup = RequestDeduplicator::<i32, &str>::new();
        let request_id = hash_request(1, Generation::next(), "fetch_runs_x_100");

        let first = dedup
            .deduplicate(request_id, || async { Err::<i32, &str>("provider error") })
            .await;
        assert_eq!(first, Err("provider error"));

        let second = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            dedup.deduplicate(request_id, || async { Ok::<_, &str>(7) }),
        )
        .await;
        assert_eq!(second, Ok(Ok(7)));
    }

    #[tokio::test]
    async fn dropped_request_does_not_block_the_next_one() {
        let dedup = RequestDeduplicator::<i32, &str>::new();
        let request_id = hash_request(1, Generation::next(), "fetch_runs_x_100");

        let abandoned = tokio::time::timeout(
            tokio::time::Duration::from_millis(20),
            dedup.deduplicate(request_id, || async {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                Ok::<_, &str>(1)
            }),
        )
        .await;
        assert!(abandoned.is_err());

        let next = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            dedup.deduplicate(request_id, || async { Ok::<_, &str>(2) }),
        )
        .await;
        assert_eq!(next, Ok(Ok(2)));
    }

    #[tokio::test]
    async fn failure_reaches_every_waiter_and_runs_once() {
        let dedup = RequestDeduplicator::<i32, &str>::new();
        let request_id = hash_request(1, Generation::next(), "fetch_runs_x_100");
        let calls = Arc::new(AtomicUsize::new(0));

        let leader = dedup.clone();
        let leader_calls = calls.clone();
        let first = tokio::spawn(async move {
            leader
                .deduplicate(request_id, || async move {
                    leader_calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    Err::<i32, &str>("provider error")
                })
                .await
        });
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let waiter_calls = calls.clone();
        let waiter = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            dedup.deduplicate(request_id, || async move {
                waiter_calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &str>(3)
            }),
        )
        .await;

        assert_eq!(first.await.unwrap(), Err("provider error"));
        assert_eq!(waiter, Ok(Err("provider error")));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_hash_request() {
        let generation = Generation::next();
        let h1 = hash_request(1, generation, "pipelines");
        let h2 = hash_request(1, generation, "pipelines");
        let h3 = hash_request(2, generation, "pipelines");
        let h4 = hash_request(1, Generation::next(), "pipelines");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
    }

    #[tokio::test]
    async fn different_generations_are_not_shared() {
        let dedup = RequestDeduplicator::<i32, &str>::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let old = hash_request(1, Generation::next(), "fetch_runs_x_100");
        let new = hash_request(1, Generation::next(), "fetch_runs_x_100");
        let run = |request_id, value| {
            let dedup = dedup.clone();
            let calls = calls.clone();
            async move {
                dedup
                    .deduplicate(request_id, || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                        Ok::<_, &str>(value)
                    })
                    .await
            }
        };

        let (first, second) = tokio::join!(run(old, 1), run(new, 2));

        assert_eq!(first, Ok(1));
        assert_eq!(second, Ok(2));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_hash_pipeline_run() {
        let h1 = hash_pipeline_run(
            1,
            "success",
            Some("main"),
            "2024-01-01T00:00:00Z",
            Some(60),
            None,
        );
        let h2 = hash_pipeline_run(
            1,
            "success",
            Some("main"),
            "2024-01-01T00:00:00Z",
            Some(60),
            None,
        );
        let h3 = hash_pipeline_run(
            1,
            "failed",
            Some("main"),
            "2024-01-01T00:00:00Z",
            Some(60),
            None,
        );

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
}
