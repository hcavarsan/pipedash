use axum::http::StatusCode;
use pipedash_core::domain::DomainError;

use super::AppError;

#[test]
fn transient_and_conflict_errors_keep_their_status_and_code() {
    let cases = [
        (
            DomainError::RateLimited {
                message: "x".into(),
                retry_after: None,
            },
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
        ),
        (
            DomainError::ServiceUnavailable {
                message: "x".into(),
                retry_after: None,
            },
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
        ),
        (
            DomainError::Timeout("x".into()),
            StatusCode::GATEWAY_TIMEOUT,
            "TIMEOUT",
        ),
        (
            DomainError::NetworkError("x".into()),
            StatusCode::BAD_GATEWAY,
            "NETWORK_ERROR",
        ),
        (
            DomainError::Tls("x".into()),
            StatusCode::BAD_GATEWAY,
            "TLS_ERROR",
        ),
        (
            DomainError::ConcurrentModification("x".into()),
            StatusCode::CONFLICT,
            "CONFLICT",
        ),
        (
            DomainError::NotSupported("x".into()),
            StatusCode::NOT_IMPLEMENTED,
            "NOT_SUPPORTED",
        ),
    ];

    for (error, status, code) in cases {
        let response = AppError::from(error);

        assert_eq!(response.status, status, "{code}");
        assert_eq!(response.error.code, code);
    }
}
