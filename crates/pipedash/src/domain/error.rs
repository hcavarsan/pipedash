use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Pipeline not found: {0}")]
    PipelineNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

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

    #[allow(dead_code)]
    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Network error: {0}")]
    NetworkError(String),
}

pub type DomainResult<T> = Result<T, DomainError>;
