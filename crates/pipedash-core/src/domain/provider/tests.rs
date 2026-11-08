use chrono::{
    DateTime,
    Duration,
    Utc,
};

use super::*;

fn now() -> DateTime<Utc> {
    "2026-10-28T12:00:00Z".parse().unwrap()
}

fn compute(
    status: FetchStatus, level: HealthLevel, fetched_ago: Option<Duration>,
    paused_in: Option<Duration>, interval: i64,
) -> ProviderFreshness {
    ProviderFreshness::compute(
        &status,
        level,
        fetched_ago.map(|ago| now() - ago),
        paused_in.map(|ahead| now() + ahead),
        interval,
        now(),
    )
}

fn plain(state: FreshnessState) -> ProviderFreshness {
    ProviderFreshness {
        state,
        stale_at: None,
    }
}

#[test]
fn error_status_is_down() {
    let f = compute(
        FetchStatus::Error,
        HealthLevel::Operational,
        Some(Duration::seconds(5)),
        None,
        30,
    );
    assert_eq!(f, plain(FreshnessState::Down));
}

#[test]
fn partial_and_major_outage_are_down_even_when_never_fetched() {
    for level in [HealthLevel::PartialOutage, HealthLevel::MajorOutage] {
        let f = compute(FetchStatus::Never, level, None, None, 30);
        assert_eq!(f, plain(FreshnessState::Down));
    }
    let f = compute(
        FetchStatus::Success,
        HealthLevel::Degraded,
        Some(Duration::seconds(5)),
        None,
        30,
    );
    assert_eq!(f.state, FreshnessState::Fresh);
}

#[test]
fn never_fetched_is_never() {
    let f = compute(FetchStatus::Never, HealthLevel::Operational, None, None, 30);
    assert_eq!(f, plain(FreshnessState::Never));
}

#[test]
fn paused_in_the_future_is_stale_but_past_pause_is_ignored() {
    let paused = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(5)),
        Some(Duration::seconds(30)),
        30,
    );
    assert_eq!(paused, plain(FreshnessState::Stale));

    let elapsed = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(5)),
        Some(Duration::seconds(-30)),
        30,
    );
    assert_eq!(elapsed.state, FreshnessState::Fresh);
}

#[test]
fn old_fetch_is_stale() {
    let f = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(601)),
        None,
        30,
    );
    assert_eq!(f, plain(FreshnessState::Stale));
}

#[test]
fn threshold_is_the_larger_of_three_intervals_and_ten_minutes() {
    let at_ten_minutes = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(600)),
        None,
        30,
    );
    assert_eq!(at_ten_minutes.state, FreshnessState::Fresh);

    let long_interval_inside = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(1500)),
        None,
        600,
    );
    assert_eq!(long_interval_inside.state, FreshnessState::Fresh);

    let long_interval_outside = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(1801)),
        None,
        600,
    );
    assert_eq!(long_interval_outside, plain(FreshnessState::Stale));
}

#[test]
fn fresh_reports_when_it_turns_stale() {
    let f = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(100)),
        None,
        600,
    );
    assert_eq!(f.state, FreshnessState::Fresh);
    assert_eq!(
        f.stale_at,
        Some(now() - Duration::seconds(100) + Duration::seconds(1800))
    );

    let short = compute(
        FetchStatus::Success,
        HealthLevel::Operational,
        Some(Duration::seconds(100)),
        None,
        30,
    );
    assert_eq!(
        short.stale_at,
        Some(now() - Duration::seconds(100) + Duration::seconds(600))
    );
}

#[test]
fn serializes_snake_case_and_defaults_to_never() {
    let json = serde_json::to_value(ProviderFreshness::default()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "state": "never", "stale_at": null })
    );
}

fn at(minutes: i64) -> ProviderFreshness {
    ProviderFreshness {
        state: FreshnessState::Fresh,
        stale_at: Some(now() + Duration::minutes(minutes)),
    }
}

#[test]
fn aggregate_worst_state_wins() {
    let aggregate = |states: &[FreshnessState]| {
        let items: Vec<ProviderFreshness> = states.iter().map(|s| plain(*s)).collect();
        ProviderFreshness::aggregate(&items).state
    };
    use FreshnessState::{
        Down,
        Fresh,
        Never,
        Stale,
    };
    assert_eq!(aggregate(&[Fresh, Never, Stale]), Stale);
    assert_eq!(aggregate(&[Fresh, Down, Stale]), Down);
    assert_eq!(aggregate(&[Fresh, Never]), Never);
    assert_eq!(aggregate(&[]), Fresh);
}

#[test]
fn aggregate_keeps_the_earliest_stale_at_only_while_fresh() {
    let fresh = ProviderFreshness::aggregate(&[at(9), at(5), at(7)]);
    assert_eq!(fresh, at(5));

    let down = ProviderFreshness::aggregate(&[at(5), plain(FreshnessState::Down)]);
    assert_eq!(down, plain(FreshnessState::Down));

    assert_eq!(
        ProviderFreshness::aggregate(&[]),
        plain(FreshnessState::Fresh)
    );
}

#[test]
fn recovery_asks_signed_in_providers_to_sign_in_again() {
    let recovery = ProviderRecovery::for_failure;
    assert_eq!(
        recovery(Some(FailureKind::Auth), true),
        Some(ProviderRecovery::SignIn)
    );
    assert_eq!(
        recovery(Some(FailureKind::Auth), false),
        Some(ProviderRecovery::Settings)
    );
    assert_eq!(
        recovery(Some(FailureKind::Tls), true),
        Some(ProviderRecovery::Settings)
    );
    assert_eq!(recovery(Some(FailureKind::Network), true), None);
    assert_eq!(recovery(None, true), None);
}
