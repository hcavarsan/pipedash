use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{
    Hash,
    Hasher,
};
use std::sync::Arc;

use sha2::{
    Digest,
    Sha256,
};
use tokio::sync::{
    oneshot,
    Mutex,
};

type RequestId = u64;

pub struct RequestDeduplicator<T: Clone> {
    #[allow(clippy::type_complexity)]
    in_flight: Arc<Mutex<HashMap<RequestId, Vec<oneshot::Sender<Arc<T>>>>>>,
}

impl<T: Clone> RequestDeduplicator<T> {
    pub fn new() -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn deduplicate<F, Fut, E>(&self, request_id: RequestId, operation: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        let (tx, rx) = oneshot::channel();

        let mut in_flight = self.in_flight.lock().await;

        if let Some(waiters) = in_flight.get_mut(&request_id) {
            waiters.push(tx);
            drop(in_flight);

            match rx.await {
                Ok(result) => Ok((*result).clone()),
                Err(_) => operation().await, // Fallback if sender dropped
            }
        } else {
            in_flight.insert(request_id, vec![tx]);
            drop(in_flight);

            let result = operation().await?;
            let shared_result = Arc::new(result.clone());

            let mut in_flight = self.in_flight.lock().await;
            if let Some(waiters) = in_flight.remove(&request_id) {
                for waiter in waiters {
                    let _ = waiter.send(shared_result.clone());
                }
            }

            Ok(result)
        }
    }
}

impl<T: Clone> Default for RequestDeduplicator<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn hash_request(provider_id: i64, endpoint: &str) -> RequestId {
    let mut hasher = DefaultHasher::new();
    provider_id.hash(&mut hasher);
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

    let result = hasher.finalize();
    format!("{:x}", result)
}

impl<T: Clone> Clone for RequestDeduplicator<T> {
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
        let dedup = RequestDeduplicator::<i32>::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let request_id = hash_request(1, "test");

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

    #[test]
    fn test_hash_request() {
        let h1 = hash_request(1, "pipelines");
        let h2 = hash_request(1, "pipelines");
        let h3 = hash_request(2, "pipelines");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
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
