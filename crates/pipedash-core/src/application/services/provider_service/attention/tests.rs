use chrono::{
    DateTime,
    Utc,
};
use serde_json::{
    json,
    Value,
};

use super::*;

const AUTH_HINT: &str = "The token was rejected. Update it in Settings → Providers.";
const UNREACHABLE: &str =
    "Pipedash can't reach this provider. Check the network or VPN of the machine running Pipedash.";
const NEXT: &str = "2026-10-22T10:05:00Z";

fn merge(mut base: Value, patch: Value) -> Value {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        base.extend(patch.clone());
    }
    base
}

fn signal(patch: Value) -> Value {
    merge(
        json!({
            "kind": "status_page", "episode_id": "ep-1", "level": "major_outage",
            "evidence": "fresh", "observed_at": "2026-10-22T10:00:00Z", "title": "API is down",
            "detail": null, "reported_by": "Stub Status", "url": null, "components": [],
            "incidents": [], "maintenances": [], "updated_at": "2026-10-22T10:00:00Z"
        }),
        patch,
    )
}

fn maintenance(patch: Value) -> Value {
    merge(
        json!({
            "id": "m1", "name": "Database upgrade", "in_progress": false, "starts_at": null,
            "ends_at": null, "url": null
        }),
        patch,
    )
}

fn health(level: &str, signals: Vec<Value>, patch: Value) -> Value {
    merge(
        json!({
            "level": level, "signals": signals, "next_attempt_at": null, "in_flight": false,
            "status_page": null
        }),
        patch,
    )
}

fn summary(patch: Value) -> ProviderSummary {
    let base = json!({
        "id": 7, "name": "Stub", "provider_type": "stub", "icon": null, "pipeline_count": 3,
        "last_updated": null, "refresh_interval": 30, "configured_repositories": [],
        "last_fetch_status": "success", "last_fetch_error": null, "last_fetch_error_kind": null,
        "last_fetch_at": null, "version": 1, "recovery": null,
        "health": health("operational", Vec::new(), json!({}))
    });
    serde_json::from_value(merge(base, patch)).unwrap()
}

fn failing(kind: Value, message: &str, health: Value) -> ProviderSummary {
    summary(json!({
        "last_fetch_status": "error", "last_fetch_error": message,
        "last_fetch_error_kind": kind, "health": health
    }))
}

fn outage(signals: Vec<Value>) -> Value {
    health("major_outage", signals, json!({}))
}

fn instant(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

#[test]
fn operational_provider_needs_no_attention_stub() {
    let result = provider_attention(&summary(json!({})));

    assert_eq!(result.tone, None);
    assert_eq!(result.headline, None);
    assert_eq!(result.hint, None);
    assert_eq!(result.episode_key, None);
    assert_eq!(result.schedule, None);
    assert!(result.refreshing_normally);
    assert_eq!(result.open_action, ProviderOpenAction::Pipelines);
}

#[test]
fn partial_outage_or_worse_is_a_warning_headed_by_the_lead_signal_stub() {
    for level in ["partial_outage", "major_outage"] {
        let result = provider_attention(&summary(json!({
            "health": health(level, vec![signal(json!({ "level": level }))], json!({}))
        })));

        assert_eq!(result.tone, Some(AttentionTone::Warning));
        assert_eq!(result.headline.as_deref(), Some("API is down"));
        assert_eq!(result.episode_key.as_deref(), Some("7:ep-1"));
    }
}

#[test]
fn an_outage_outranks_a_non_transient_fetch_error_stub() {
    let result = provider_attention(&failing(
        json!("auth"),
        "bad token",
        outage(vec![signal(json!({}))]),
    ));

    assert_eq!(result.tone, Some(AttentionTone::Warning));
    assert_eq!(result.hint.as_deref(), Some(AUTH_HINT));
}

#[test]
fn non_transient_fetch_errors_are_danger_even_with_a_minor_status_issue_stub() {
    for kind in ["auth", "permission", "config", "tls", "other", "not_found"] {
        let degraded = health(
            "degraded",
            vec![signal(json!({ "level": "degraded", "title": "Slow" }))],
            json!({}),
        );
        let result = provider_attention(&failing(json!(kind), "boom", degraded));

        assert_eq!(result.tone, Some(AttentionTone::Danger), "{kind}");
        assert_eq!(result.headline.as_deref(), Some("Fetch failed"));
        assert_eq!(result.detail.as_deref(), Some("boom"));
        assert_eq!(
            result.kind,
            Some(serde_json::from_value(json!(kind)).unwrap())
        );
        assert_eq!(
            result.episode_key.as_deref(),
            Some(format!("7:fetch-failed:{kind}").as_str())
        );
    }
}

#[test]
fn a_fetch_error_without_a_kind_is_non_transient_stub() {
    let result = provider_attention(&failing(
        Value::Null,
        "unknown",
        health("operational", Vec::new(), json!({})),
    ));

    assert_eq!(result.tone, Some(AttentionTone::Danger));
    assert_eq!(result.episode_key.as_deref(), Some("7:fetch-failed:other"));
    assert_eq!(result.kind, None);
}

#[test]
fn transient_fetch_errors_alone_do_not_raise_attention_stub() {
    for kind in ["network", "timeout", "rate_limited", "unavailable"] {
        let result = provider_attention(&failing(
            json!(kind),
            "x",
            health("operational", Vec::new(), json!({})),
        ));

        assert_eq!(result.tone, None, "{kind}");
        assert_eq!(result.schedule, None);
    }
}

#[test]
fn degraded_maintenance_and_rate_limits_do_not_raise_attention_stub() {
    let quiet = [
        health(
            "degraded",
            vec![signal(json!({ "level": "degraded" }))],
            json!({}),
        ),
        health(
            "degraded",
            vec![signal(json!({ "kind": "rate_limit", "level": "degraded" }))],
            json!({}),
        ),
        health(
            "maintenance",
            vec![signal(json!({
                "level": "maintenance",
                "maintenances": [maintenance(json!({ "in_progress": true }))]
            }))],
            json!({}),
        ),
        health(
            "operational",
            vec![signal(json!({
                "level": "operational",
                "maintenances": [maintenance(json!({ "starts_at": "2099-01-01T00:00:00Z" }))]
            }))],
            json!({}),
        ),
    ];

    for value in quiet {
        let result = provider_attention(&summary(json!({ "health": value })));

        assert_eq!(result.tone, None);
    }
}

#[test]
fn stale_signals_do_not_set_the_tone_and_are_exposed_stub() {
    let stale = signal(json!({ "evidence": "stale", "title": "API is down" }));
    let result = provider_attention(&summary(json!({
        "health": health("operational", vec![stale.clone()], json!({}))
    })));

    assert_eq!(result.tone, None);
    assert_eq!(result.stale_signals.len(), 1);
    assert_eq!(result.stale_signals[0].title, "API is down");
    assert_eq!(result.stale_signals[0].kind, SignalKind::StatusPage);
}

#[test]
fn stale_signals_never_lead_an_outage_stub() {
    let stale = signal(json!({ "evidence": "stale" }));
    let result = provider_attention(&summary(json!({ "health": outage(vec![stale]) })));

    assert_eq!(result.tone, None);
    assert_eq!(result.stale_signals.len(), 1);
}

#[test]
fn hints_follow_the_failure_kind_and_fall_back_to_the_raw_message_stub() {
    let hint_for = |kind: Value, message: &str| {
        provider_attention(&failing(kind, message, outage(vec![signal(json!({}))]))).hint
    };
    let raw = "raw message";

    assert_eq!(hint_for(json!("auth"), raw).as_deref(), Some(AUTH_HINT));
    assert_eq!(
        hint_for(json!("permission"), raw).as_deref(),
        Some("The token is missing permissions. Check them in Settings → Providers.")
    );
    for kind in ["config", "tls"] {
        assert_eq!(
            hint_for(json!(kind), raw).as_deref(),
            Some("Check the provider URL and certificate settings.")
        );
    }
    for kind in ["network", "timeout"] {
        assert_eq!(hint_for(json!(kind), raw).as_deref(), Some(UNREACHABLE));
    }
    assert_eq!(
        hint_for(json!("rate_limited"), raw).as_deref(),
        Some("Rate limited.")
    );
    assert_eq!(
        hint_for(json!("unavailable"), raw).as_deref(),
        Some("The provider is returning server errors.")
    );
    assert_eq!(hint_for(json!("other"), raw).as_deref(), Some(raw));
    assert_eq!(hint_for(json!("not_found"), raw).as_deref(), Some(raw));
    assert_eq!(hint_for(Value::Null, "no kind").as_deref(), Some("no kind"));
}

#[test]
fn sign_in_recovery_replaces_the_hint_stub() {
    let mut failed = failing(json!("auth"), "expired", outage(vec![signal(json!({}))]));
    failed.recovery = Some(ProviderRecovery::SignIn);

    let result = provider_attention(&failed);

    assert_eq!(
        result.hint.as_deref(),
        Some("Your sign-in expired. Sign in again to reconnect.")
    );
    assert_eq!(result.recovery, Some(ProviderRecovery::SignIn));
    assert_eq!(result.open_action, ProviderOpenAction::SignIn);
}

#[test]
fn fetch_hint_is_always_computed_and_empty_without_a_failure_stub() {
    let explained = failing(
        json!("network"),
        "down",
        outage(vec![signal(json!({ "kind": "connection" }))]),
    );
    let result = provider_attention(&explained);

    assert_eq!(result.hint, None);
    assert_eq!(result.fetch_hint.as_deref(), Some(UNREACHABLE));
    assert_eq!(
        provider_attention(&summary(
            json!({ "health": outage(vec![signal(json!({}))]) })
        ))
        .fetch_hint,
        None
    );
}

#[test]
fn kind_is_reported_only_for_a_failed_fetch_stub() {
    let result = provider_attention(&summary(json!({ "last_fetch_error_kind": "auth" })));

    assert_eq!(result.kind, None);
}

#[test]
fn next_attempt_and_checking_now_are_reported_as_schedule_stub() {
    let connection = vec![signal(json!({ "kind": "connection" }))];
    let on = |extra: Value| {
        provider_attention(&failing(
            json!("network"),
            "down",
            health("major_outage", connection.clone(), extra),
        ))
    };

    let waiting = on(json!({ "next_attempt_at": NEXT }));
    assert_eq!(
        waiting.schedule,
        Some(AttentionSchedule::NextAttempt { at: instant(NEXT) })
    );
    assert_eq!(waiting.hint, None);

    let checking = on(json!({ "in_flight": true, "next_attempt_at": NEXT }));
    assert_eq!(checking.schedule, Some(AttentionSchedule::CheckingNow));
}

#[test]
fn a_provider_outage_without_a_fetch_error_has_only_the_schedule_stub() {
    let quiet = provider_attention(&summary(
        json!({ "health": outage(vec![signal(json!({}))]) }),
    ));
    let checking = provider_attention(&summary(json!({
        "health": health("major_outage", vec![signal(json!({}))], json!({ "in_flight": true }))
    })));

    assert_eq!(quiet.hint, None);
    assert_eq!(quiet.schedule, None);
    assert_eq!(checking.hint, None);
    assert_eq!(checking.schedule, Some(AttentionSchedule::CheckingNow));
}

#[test]
fn schedule_needs_a_lead_stub() {
    let waiting = summary(json!({
        "health": health("operational", Vec::new(), json!({ "next_attempt_at": NEXT, "in_flight": true }))
    }));

    assert_eq!(provider_attention(&waiting).schedule, None);
}

#[test]
fn refreshing_normally_needs_a_successful_fetch_no_connection_signal_and_no_backoff_stub() {
    let normal = || summary(json!({ "health": outage(vec![signal(json!({}))]) }));

    assert!(provider_attention(&normal()).refreshing_normally);

    let mut never = normal();
    never.last_fetch_status = FetchStatus::Never;
    assert!(!provider_attention(&never).refreshing_normally);

    let mut error = normal();
    error.last_fetch_status = FetchStatus::Error;
    assert!(!provider_attention(&error).refreshing_normally);

    let connection = summary(json!({
        "health": outage(vec![
            signal(json!({})),
            signal(json!({ "kind": "connection", "episode_id": "c1" })),
        ])
    }));
    assert!(!provider_attention(&connection).refreshing_normally);

    let backoff = summary(json!({
        "health": health("major_outage", vec![signal(json!({}))], json!({ "next_attempt_at": NEXT }))
    }));
    assert!(!provider_attention(&backoff).refreshing_normally);
}

#[test]
fn outage_levels_start_at_partial_outage_stub() {
    assert!(!is_outage_level(HealthLevel::Degraded));
    assert!(is_outage_level(HealthLevel::PartialOutage));
    assert!(is_outage_level(HealthLevel::MajorOutage));
}

#[test]
fn open_action_follows_recovery_failure_freshness_and_outage_stub() {
    let action = |patch: Value| provider_attention(&summary(patch)).open_action;

    assert_eq!(action(json!({})), ProviderOpenAction::Pipelines);
    assert_eq!(
        action(json!({ "last_fetch_status": "error" })),
        ProviderOpenAction::Status
    );
    assert_eq!(
        action(json!({ "freshness": { "state": "down", "stale_at": null } })),
        ProviderOpenAction::Status
    );
    assert_eq!(
        action(json!({ "health": outage(Vec::new()) })),
        ProviderOpenAction::Status
    );
    assert_eq!(
        action(json!({ "health": health("degraded", Vec::new(), json!({})) })),
        ProviderOpenAction::Pipelines
    );
}

#[test]
fn scheduled_maintenances_come_from_fresh_signals_not_started_stub() {
    let upcoming = maintenance(json!({ "id": "soon", "starts_at": "2099-01-01T00:00:00Z" }));
    let running = maintenance(json!({
        "id": "now", "in_progress": true, "starts_at": "2026-10-22T09:00:00Z"
    }));
    let undated = maintenance(json!({ "id": "undated" }));
    let hidden = maintenance(json!({ "id": "stale", "starts_at": "2099-02-01T00:00:00Z" }));
    let signals = vec![
        signal(json!({
            "level": "operational", "maintenances": [upcoming, running, undated]
        })),
        signal(json!({
            "evidence": "stale", "episode_id": "ep-2", "maintenances": [hidden]
        })),
    ];

    let result = provider_attention(&summary(json!({
        "health": health("operational", signals, json!({}))
    })));

    let ids: Vec<&str> = result
        .scheduled_maintenances
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(ids, vec!["soon"]);
}

#[test]
fn attention_serializes_snake_case_stub() {
    let result = provider_attention(&failing(
        json!("auth"),
        "bad",
        health(
            "operational",
            Vec::new(),
            json!({ "next_attempt_at": NEXT }),
        ),
    ));
    let value = serde_json::to_value(&result).unwrap();

    assert_eq!(value["tone"], "danger");
    assert_eq!(value["open_action"], "status");
    assert_eq!(
        value["schedule"],
        json!({ "type": "next_attempt", "at": NEXT })
    );
    assert_eq!(value["kind"], "auth");
}
