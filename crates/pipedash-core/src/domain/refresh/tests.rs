use super::*;

#[test]
fn serializes_to_the_wire_contract() {
    let result = RefreshResult::failed(7, FailureKind::RateLimited, "slow down".to_string());

    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::json!({
            "provider_id": 7,
            "state": "failed",
            "kind": "rate_limited",
            "message": "slow down"
        })
    );
    assert_eq!(
        serde_json::to_value(RefreshResult::new(1, RefreshState::NotFound)).unwrap(),
        serde_json::json!({
            "provider_id": 1,
            "state": "not_found",
            "kind": null,
            "message": null
        })
    );
}
