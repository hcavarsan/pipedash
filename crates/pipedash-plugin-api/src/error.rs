use thiserror::Error;

/// Plugin error types
#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

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
}

pub type PluginResult<T> = Result<T, PluginError>;

// Conversion from serde_json errors
impl From<serde_json::Error> for PluginError {
    fn from(err: serde_json::Error) -> Self {
        PluginError::SerializationError(err.to_string())
    }
}
