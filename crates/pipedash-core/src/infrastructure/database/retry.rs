use std::future::Future;
use std::time::Duration;

use tokio::time::sleep;

use crate::domain::{
    DomainError,
    DomainResult,
};

pub(super) async fn retry_on_busy<F, Fut, T>(operation: F) -> DomainResult<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = DomainResult<T>>,
{
    const MAX_RETRIES: u32 = 3;
    const INITIAL_DELAY_MS: u64 = 10;

    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::warn!(
                        "SQLite operation succeeded after {} retries ({}ms total)",
                        attempt,
                        INITIAL_DELAY_MS * (2_u64.pow(attempt) - 1)
                    );
                }
                return Ok(result);
            }
            Err(DomainError::DatabaseError(msg))
                if (msg.contains("database is locked")
                    || msg.contains("SQLITE_BUSY")
                    || msg.contains("locked"))
                    && attempt < MAX_RETRIES =>
            {
                attempt += 1;
                let delay = INITIAL_DELAY_MS * 2_u64.pow(attempt - 1);

                if attempt > 3 {
                    tracing::warn!(
                        "SQLite busy, retry {}/{} (waiting {}ms)",
                        attempt,
                        MAX_RETRIES,
                        delay
                    );
                }

                sleep(Duration::from_millis(delay)).await;
            }
            Err(e) => {
                if attempt > 0 {
                    tracing::error!("SQLite operation failed after {} retries: {}", attempt, e);
                }
                return Err(e);
            }
        }
    }
}
