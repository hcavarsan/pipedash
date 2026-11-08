use super::*;

#[test]
fn relay_statuses_map_to_outcomes() {
    assert_eq!(relay_outcome(202), RelayOutcome::Delivered);
    assert_eq!(relay_outcome(404), RelayOutcome::Gone);
    assert_eq!(relay_outcome(410), RelayOutcome::Gone);
    assert_eq!(relay_outcome(429), RelayOutcome::RateLimited);
    assert_eq!(relay_outcome(400), RelayOutcome::Failed);
    assert_eq!(relay_outcome(502), RelayOutcome::Failed);
}

#[test]
fn relay_urls_are_trimmed_and_blank_means_none() {
    assert_eq!(
        normalize_relay_url(" https://relay.example.com/ ").as_deref(),
        Some("https://relay.example.com")
    );
    assert_eq!(normalize_relay_url("  /  "), None);
    assert_eq!(normalize_relay_url(""), None);
}
