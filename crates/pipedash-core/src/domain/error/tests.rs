use super::*;

#[test]
fn failure_kind_classifies_transient_errors() {
    let transient = [
        DomainError::NetworkError("x".into()),
        DomainError::Timeout("x".into()),
        DomainError::ServiceUnavailable {
            message: "x".into(),
            retry_after: None,
        },
        DomainError::RateLimited {
            message: "x".into(),
            retry_after: Some(Duration::from_secs(5)),
        },
    ];
    for err in transient {
        assert!(err.failure_kind().is_transient(), "{err:?}");
    }

    let permanent = [
        (DomainError::Tls("x".into()), FailureKind::Tls),
        (
            DomainError::ProviderAuthenticationFailed("x".into()),
            FailureKind::Auth,
        ),
        (
            DomainError::PermissionDenied("x".into()),
            FailureKind::Permission,
        ),
        (DomainError::InvalidConfig("x".into()), FailureKind::Config),
        (
            DomainError::PipelineNotFound("x".into()),
            FailureKind::NotFound,
        ),
        (DomainError::ApiError("x".into()), FailureKind::Other),
    ];
    for (err, kind) in permanent {
        assert_eq!(err.failure_kind(), kind);
        assert!(!kind.is_transient());
    }
}

#[test]
fn codes_are_stable_per_variant() {
    let cases = [
        (DomainError::ProviderNotFound("x".into()), "NOT_FOUND"),
        (DomainError::PipelineNotFound("x".into()), "NOT_FOUND"),
        (DomainError::NotFound("x".into()), "NOT_FOUND"),
        (DomainError::InvalidConfig("x".into()), "BAD_REQUEST"),
        (DomainError::InvalidProviderType("x".into()), "BAD_REQUEST"),
        (
            DomainError::AuthenticationFailed("x".into()),
            "UNAUTHORIZED",
        ),
        (
            DomainError::ProviderAuthenticationFailed("x".into()),
            "PROVIDER_AUTH_FAILED",
        ),
        (DomainError::PermissionDenied("x".into()), "FORBIDDEN"),
        (DomainError::NotSupported("x".into()), "NOT_SUPPORTED"),
        (DomainError::ApiError("x".into()), "INTERNAL_ERROR"),
        (DomainError::ConfigNotFound, "INTERNAL_ERROR"),
        (
            DomainError::RateLimited {
                message: "x".into(),
                retry_after: None,
            },
            "INTERNAL_ERROR",
        ),
    ];
    for (err, code) in cases {
        assert_eq!(err.code(), code, "{err:?}");
    }
}
