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
