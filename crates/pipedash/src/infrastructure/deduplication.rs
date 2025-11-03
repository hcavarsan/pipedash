use std::collections::HashMap;
use std::hash::{
    Hash,
    Hasher,
};
use std::sync::Arc;

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
                Err(_) => operation().await,
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
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    provider_id.hash(&mut hasher);
    endpoint.hash(&mut hasher);
    hasher.finish()
}

pub fn hash_pipeline_run(
    run_number: i64, status: &str, branch: Option<&str>, started_at: &str,
    duration_seconds: Option<i64>, commit_sha: Option<&str>,
) -> String {
    use sha2::{
        Digest,
        Sha256,
    };

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
