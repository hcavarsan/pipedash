use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::RateLimit;
use tokio::time::{
    timeout,
    Instant,
};

use super::ProviderLease;
use super::ProviderService;

const QUOTA_PAUSE_PERCENT: u64 = 10;

impl ProviderService {
    pub async fn check_quota(&self, lease: &ProviderLease) -> Option<DateTime<Utc>> {
        let rate_limit =
            match timeout(Duration::from_secs(10), lease.provider.fetch_rate_limit()).await {
                Ok(Ok(rate_limit)) => rate_limit,
                Ok(Err(e)) => {
                    tracing::debug!(provider_id = lease.id, error = %e, "Rate limit check failed");
                    None
                }
                Err(_) => {
                    tracing::debug!(provider_id = lease.id, "Rate limit check timed out");
                    None
                }
            };
        let paused_until = rate_limit.and_then(|rate_limit| quota_pause(&rate_limit, Utc::now()));

        self.commit(lease, || async {
            let changed = self.scheduler.set_quota_pause(
                lease.id,
                lease.generation,
                paused_until,
                Instant::now(),
                Utc::now(),
            );
            if let (true, Some(until)) = (changed, paused_until) {
                tracing::info!(provider_id = lease.id, %until, "API quota almost used up, pausing provider");
            }
            if changed {
                self.emit_status(lease.id).await;
            }

            paused_until
        })
        .await
        .flatten()
    }
}

pub(crate) fn quota_pause(rate_limit: &RateLimit, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let low = rate_limit.remaining * 100 < rate_limit.limit * QUOTA_PAUSE_PERCENT;

    (low && rate_limit.reset_at > now).then_some(rate_limit.reset_at)
}
