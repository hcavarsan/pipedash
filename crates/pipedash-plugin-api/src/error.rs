use std::time::Duration;

use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum PluginError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Pipeline not found: {0}")]
    PipelineNotFound(String),

    #[error("Provider not supported: {0}")]
    ProviderNotSupported(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Rate limited: {message}")]
    RateLimited {
        message: String,
        retry_after: Option<Duration>,
    },

    #[error("Service unavailable: {message}")]
    ServiceUnavailable {
        message: String,
        retry_after: Option<Duration>,
    },

    #[error("TLS error: {0}")]
    Tls(String),
}

pub type PluginResult<T> = Result<T, PluginError>;

impl PluginError {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::NetworkError(_) | Self::ServiceUnavailable { .. } | Self::RateLimited { .. }
        )
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after, .. }
            | Self::ServiceUnavailable { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(err: serde_json::Error) -> Self {
        PluginError::SerializationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_covers_network_unavailable_and_rate_limit_only() {
        assert!(PluginError::NetworkError("x".into()).is_transient());
        assert!(PluginError::RateLimited {
            message: "x".into(),
            retry_after: None
        }
        .is_transient());
        assert!(PluginError::ServiceUnavailable {
            message: "x".into(),
            retry_after: None
        }
        .is_transient());
        assert!(!PluginError::ApiError("x".into()).is_transient());
        assert!(!PluginError::AuthenticationFailed("x".into()).is_transient());
        assert!(!PluginError::Tls("x".into()).is_transient());
    }
}
