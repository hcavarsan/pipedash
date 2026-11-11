use std::time::Duration;

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DomainError {
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Pipeline not found: {0}")]
    PipelineNotFound(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authentication failed: {0}")]
    ProviderAuthenticationFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Invalid provider type: {0}")]
    InvalidProviderType(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Not supported: {0}")]
    NotSupported(String),

    #[error("Internal error: {0}")]
    InternalError(String),

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

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Lock error: {0}")]
    LockError(String),

    #[error("Concurrent modification: {0}")]
    ConcurrentModification(String),

    #[error("Configuration file not found - setup wizard required")]
    ConfigNotFound,

    #[error("Configuration file is invalid: {path} - {errors}")]
    InvalidConfigFile { path: String, errors: String },

    #[error("Application not initialized - run setup first")]
    NotInitialized,

    #[error(
        "Migration failed at step '{step}': {reason}\n\nRECOVERY INSTRUCTIONS:\n{instructions}"
    )]
    MigrationFailed {
        step: String,
        reason: String,
        instructions: String,
    },

    #[error("Data consistency error: {0}")]
    DataConsistency(String),

    #[error("{message}")]
    Unprocessable { code: &'static str, message: String },

    #[error("{message}")]
    Conflict { code: &'static str, message: String },
}

pub type DomainResult<T> = Result<T, DomainError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Network,
    Timeout,
    Tls,
    RateLimited,
    Unavailable,
    Auth,
    Permission,
    Config,
    NotFound,
    Other,
}

impl FailureKind {
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            Self::Network | Self::Timeout | Self::Unavailable | Self::RateLimited
        )
    }
}

impl DomainError {
    pub fn failure_kind(&self) -> FailureKind {
        match self {
            Self::NetworkError(_) => FailureKind::Network,
            Self::Timeout(_) => FailureKind::Timeout,
            Self::Tls(_) => FailureKind::Tls,
            Self::RateLimited { .. } => FailureKind::RateLimited,
            Self::ServiceUnavailable { .. } => FailureKind::Unavailable,
            Self::AuthenticationFailed(_) | Self::ProviderAuthenticationFailed(_) => {
                FailureKind::Auth
            }
            Self::PermissionDenied(_) => FailureKind::Permission,
            Self::InvalidConfig(_)
            | Self::InvalidProviderType(_)
            | Self::InvalidConfigFile { .. }
            | Self::ConfigNotFound => FailureKind::Config,
            Self::ProviderNotFound(_) | Self::PipelineNotFound(_) | Self::NotFound(_) => {
                FailureKind::NotFound
            }
            _ => FailureKind::Other,
        }
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after, .. }
            | Self::ServiceUnavailable { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::ProviderNotFound(_) | Self::PipelineNotFound(_) | Self::NotFound(_) => {
                "NOT_FOUND"
            }
            Self::InvalidConfig(_) | Self::InvalidProviderType(_) => "BAD_REQUEST",
            Self::AuthenticationFailed(_) => "UNAUTHORIZED",
            Self::ProviderAuthenticationFailed(_) => "PROVIDER_AUTH_FAILED",
            Self::PermissionDenied(_) => "FORBIDDEN",
            Self::NotSupported(_) => "NOT_SUPPORTED",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            Self::Timeout(_) => "TIMEOUT",
            Self::NetworkError(_) => "NETWORK_ERROR",
            Self::Tls(_) => "TLS_ERROR",
            Self::ConcurrentModification(_) => "CONFLICT",
            Self::Unprocessable { code, .. } | Self::Conflict { code, .. } => code,
            _ => "INTERNAL_ERROR",
        }
    }
}

#[cfg(test)]
mod tests;
