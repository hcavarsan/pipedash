//! Common utilities for plugin implementations

use std::time::Duration;

use crate::{
    PluginError,
    PluginResult,
};

/// Retry policy configuration
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Whether to use exponential backoff
    pub exponential_backoff: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            exponential_backoff: true,
        }
    }
}

impl RetryPolicy {
    /// Creates a new retry policy with custom settings
    pub fn new(max_retries: usize, initial_delay: Duration, exponential_backoff: bool) -> Self {
        Self {
            max_retries,
            initial_delay,
            exponential_backoff,
        }
    }

    /// Executes an operation with retry logic
    ///
    /// # Example
    ///
    /// ```ignore
    /// use pipedash_plugin_api::utils::RetryPolicy;
    ///
    /// let policy = RetryPolicy::default();
    /// let result = policy.retry(|| async {
    ///     // Your async operation here
    ///     Ok(())
    /// }).await?;
    /// ```
    pub async fn retry<F, Fut, T>(&self, operation: F) -> PluginResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = PluginResult<T>>,
    {
        let mut delay = self.initial_delay;
        let mut last_error = None;

        for attempt in 0..self.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < self.max_retries - 1 => {
                    // Only retry on network and API errors
                    match &e {
                        PluginError::NetworkError(_) | PluginError::ApiError(_) => {
                            last_error = Some(e);
                            tokio::time::sleep(delay).await;
                            if self.exponential_backoff {
                                delay *= 2;
                            }
                            continue;
                        }
                        _ => return Err(e),
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| PluginError::NetworkError("Max retries exceeded".to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_success() {
        let policy = RetryPolicy::default();
        let result = policy.retry(|| async { Ok::<_, PluginError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_eventual_success() {
        let policy = RetryPolicy::new(3, Duration::from_millis(10), false);
        let attempts = std::cell::Cell::new(0);

        let result = policy
            .retry(|| async {
                let count = attempts.get() + 1;
                attempts.set(count);
                if count < 2 {
                    Err(PluginError::NetworkError("Temporary failure".to_string()))
                } else {
                    Ok(42)
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
    }
}
