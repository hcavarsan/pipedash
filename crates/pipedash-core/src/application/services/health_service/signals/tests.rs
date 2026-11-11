use super::*;

fn observed(now: Instant) -> Observed {
    Observed {
        at: now,
        wall: Utc::now(),
    }
}

#[test]
fn evidence_ages_from_fresh_to_stale_to_dropped() {
    let start = Instant::now();
    let seen = Some(observed(start));
    let at = |minutes: u64| Evidence::of(seen, None, start + Duration::from_secs(minutes * 60));

    assert!(matches!(at(0), Some(Evidence::Fresh(_))));
    assert!(matches!(at(14), Some(Evidence::Fresh(_))));
    assert!(matches!(at(15), Some(Evidence::Stale(_))));
    assert!(matches!(at(59), Some(Evidence::Stale(_))));
    assert_eq!(at(60), None);
    assert_eq!(Evidence::of(None, None, start), None);
}

#[test]
fn unavailable_wins_over_an_older_observation_and_is_never_shown() {
    let now = Instant::now();

    let evidence = Evidence::of(
        Some(observed(now)),
        Some(UnavailableReason::PermissionDenied),
        now,
    )
    .unwrap();

    assert_eq!(
        evidence,
        Evidence::Unavailable(UnavailableReason::PermissionDenied)
    );
    assert!(!evidence.is_fresh());
    assert!(evidence.shown().is_none());
    assert!(Evidence::Suspended.shown().is_none());
    assert!(Evidence::Fresh(observed(now)).is_fresh());
}

#[test]
fn the_next_evidence_change_follows_the_observation() {
    let start = Instant::now();
    let seen = Some(observed(start));

    assert_eq!(next_evidence_change(seen, start), Some(start + FRESH_FOR));
    assert_eq!(
        next_evidence_change(seen, start + FRESH_FOR),
        Some(start + DROP_AFTER)
    );
    assert_eq!(next_evidence_change(seen, start + DROP_AFTER), None);
    assert_eq!(next_evidence_change(None, start), None);
}

#[test]
fn connection_signals_follow_the_failure_table() {
    let signal = |kind, failures| connection("Stub", kind, failures);

    assert_eq!(signal(FailureKind::Network, 1), None);
    assert_eq!(signal(FailureKind::Timeout, 1), None);
    assert_eq!(signal(FailureKind::Unavailable, 1), None);
    for kind in [FailureKind::Network, FailureKind::Timeout] {
        let major = signal(kind, 2).unwrap();
        assert_eq!(major.level, HealthLevel::MajorOutage);
        assert_eq!(major.kind, SignalKind::Connection);
        assert_eq!(major.title, "Can't reach Stub");
        assert_eq!(major.detail.as_deref(), Some(CONNECTION_DETAIL));
    }
    let partial = signal(FailureKind::Unavailable, 2).unwrap();
    assert_eq!(partial.level, HealthLevel::PartialOutage);
    assert_eq!(partial.title, "Stub is returning server errors");
    assert_eq!(partial.detail, None);
    let limited = signal(FailureKind::RateLimited, 1).unwrap();
    assert_eq!(limited.level, HealthLevel::Degraded);
    assert_eq!(limited.kind, SignalKind::RateLimit);
    assert_eq!(limited.title, "Rate limited by Stub");
    for kind in [
        FailureKind::Auth,
        FailureKind::Permission,
        FailureKind::Config,
        FailureKind::Tls,
        FailureKind::NotFound,
        FailureKind::Other,
    ] {
        assert_eq!(signal(kind, 9), None, "{kind:?}");
    }
}

#[test]
fn an_instance_signal_lists_only_broken_components_worst_first() {
    let component = |name: &str, level, detail: Option<&str>| ComponentHealth {
        name: name.to_owned(),
        level,
        detail: detail.map(str::to_owned),
    };
    let now = observed(Instant::now());

    let single = instance(
        "Stub Two",
        &[
            component(
                "kustomize-controller",
                HealthLevel::MajorOutage,
                Some("scaled to zero"),
            ),
            component("helm-controller", HealthLevel::Operational, None),
        ],
        "e1",
        EvidenceView::Fresh,
        now,
    )
    .unwrap();
    let several = instance(
        "Stub Two",
        &[
            component("a", HealthLevel::Degraded, None),
            component("b", HealthLevel::MajorOutage, None),
        ],
        "e2",
        EvidenceView::Stale,
        now,
    )
    .unwrap();

    assert_eq!(single.title, "kustomize-controller: scaled to zero");
    assert_eq!(single.components.len(), 1);
    assert_eq!(single.level, HealthLevel::MajorOutage);
    assert_eq!(several.title, "2 components need attention");
    assert_eq!(several.components[0].name, "b");
    assert_eq!(several.evidence, EvidenceView::Stale);
    assert!(instance("Stub Two", &[], "e3", EvidenceView::Fresh, now).is_none());
}
