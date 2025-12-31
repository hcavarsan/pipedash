use axum::{
    http::StatusCode,
    response::{
        IntoResponse,
        Response,
    },
    Json,
};
use pipedash_core::domain::DomainError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }
}

pub struct AppError {
    pub status: StatusCode,
    pub error: ApiError,
}

impl AppError {
    pub fn new(status: StatusCode, error: ApiError) -> Self {
        Self { status, error }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, ApiError::new("NOT_FOUND", message))
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiError::new("BAD_REQUEST", message),
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("INTERNAL_ERROR", message),
        )
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ApiError::new("UNAUTHORIZED", message),
        )
    }

    pub fn not_initialized() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                "NOT_INITIALIZED",
                "Application not initialized - setup required",
            ),
        )
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, Json(self.error)).into_response()
    }
}

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        match &err {
            DomainError::ProviderNotFound(_) => AppError::not_found(err.to_string()),
            DomainError::PipelineNotFound(_) => AppError::not_found(err.to_string()),
            DomainError::InvalidConfig(_) => AppError::bad_request(err.to_string()),
            DomainError::AuthenticationFailed(_) => AppError::unauthorized(err.to_string()),
            DomainError::InvalidProviderType(_) => AppError::bad_request(err.to_string()),
            DomainError::NotSupported(_) => AppError::new(
                StatusCode::NOT_IMPLEMENTED,
                ApiError::new("NOT_SUPPORTED", err.to_string()),
            ),
            _ => AppError::internal(err.to_string()),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::internal(err.to_string())
    }
}

pub type ApiResult<T> = Result<T, AppError>;
