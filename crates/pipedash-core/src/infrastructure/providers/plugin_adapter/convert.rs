use pipedash_plugin_api::{
    PipelineStatus as PluginStatus,
    PluginError,
};

use super::PluginAdapter;
use crate::domain::{
    DomainError,
    PipelineStatus,
};

impl PluginAdapter {
    pub(crate) fn map_error(e: PluginError) -> DomainError {
        match e {
            PluginError::AuthenticationFailed(msg) => {
                DomainError::ProviderAuthenticationFailed(msg)
            }
            PluginError::PermissionDenied(msg) => DomainError::PermissionDenied(msg),
            PluginError::ApiError(msg) => DomainError::ApiError(msg),
            PluginError::InvalidConfig(msg) => DomainError::InvalidConfig(msg),
            PluginError::PipelineNotFound(msg) => DomainError::PipelineNotFound(msg),
            PluginError::ProviderNotSupported(msg) => DomainError::InvalidProviderType(msg),
            PluginError::NetworkError(msg) => DomainError::NetworkError(msg),
            PluginError::SerializationError(msg) => {
                DomainError::ApiError(format!("Serialization error: {}", msg))
            }
            PluginError::DatabaseError(msg) => DomainError::DatabaseError(msg),
            PluginError::NotSupported(msg) => DomainError::NotSupported(msg),
            PluginError::Internal(msg) => DomainError::InternalError(msg),
            PluginError::RateLimited {
                message,
                retry_after,
            } => DomainError::RateLimited {
                message,
                retry_after,
            },
            PluginError::ServiceUnavailable {
                message,
                retry_after,
            } => DomainError::ServiceUnavailable {
                message,
                retry_after,
            },
            PluginError::Tls(msg) => DomainError::Tls(msg),
            e => DomainError::ApiError(e.to_string()),
        }
    }

    pub(super) fn convert_status(status: PluginStatus) -> PipelineStatus {
        match status {
            PluginStatus::Success => PipelineStatus::Success,
            PluginStatus::Failed => PipelineStatus::Failed,
            PluginStatus::Degraded => PipelineStatus::Degraded,
            PluginStatus::Running => PipelineStatus::Running,
            PluginStatus::Pending => PipelineStatus::Pending,
            PluginStatus::Cancelled => PipelineStatus::Cancelled,
            PluginStatus::Skipped => PipelineStatus::Skipped,
        }
    }
}
