use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    ComponentHealth,
    StatusSource,
};
use reqwest::Url;
use tokio::time::Instant;

use crate::domain::{
    EvidenceView,
    FailureKind,
    HealthLevel,
    HealthSignal,
    SignalKind,
};
use crate::infrastructure::status_feeds::FeedSignal;

pub(super) const FRESH_FOR: Duration = Duration::from_secs(15 * 60);
pub(super) const DROP_AFTER: Duration = Duration::from_secs(60 * 60);

const CONNECTION_DETAIL: &str = "Pipedash's connection to it is failing. Check the network or VPN of the machine running Pipedash.";
const PIPEDASH: &str = "Pipedash";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Observed {
    pub at: Instant,
    pub wall: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnavailableReason {
    FeedError,
    PermissionDenied,
    NotInstalled,
}

impl UnavailableReason {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::FeedError => "the status page can't be read",
            Self::PermissionDenied => "permission denied",
            Self::NotInstalled => "not installed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Evidence {
    Fresh(Observed),
    Stale(Observed),
    Unavailable(UnavailableReason),
    Suspended,
}

impl Evidence {
    pub(super) fn of(
        observed: Option<Observed>, unavailable: Option<UnavailableReason>, now: Instant,
    ) -> Option<Self> {
        if let Some(reason) = unavailable {
            return Some(Self::Unavailable(reason));
        }
        let observed = observed?;
        let age = now.saturating_duration_since(observed.at);
        if age < FRESH_FOR {
            Some(Self::Fresh(observed))
        } else if age < DROP_AFTER {
            Some(Self::Stale(observed))
        } else {
            None
        }
    }

    pub(super) fn is_fresh(self) -> bool {
        matches!(self, Self::Fresh(_))
    }

    pub(super) fn shown(self) -> Option<(EvidenceView, Observed)> {
        match self {
            Self::Fresh(observed) => Some((EvidenceView::Fresh, observed)),
            Self::Stale(observed) => Some((EvidenceView::Stale, observed)),
            Self::Unavailable(_) | Self::Suspended => None,
        }
    }
}

pub(super) fn next_evidence_change(observed: Option<Observed>, now: Instant) -> Option<Instant> {
    let observed = observed?;
    let age = now.saturating_duration_since(observed.at);
    if age < FRESH_FOR {
        Some(observed.at + FRESH_FOR)
    } else if age < DROP_AFTER {
        Some(observed.at + DROP_AFTER)
    } else {
        None
    }
}

pub(super) fn level_label(level: HealthLevel) -> &'static str {
    match level {
        HealthLevel::Operational => "all systems operational",
        HealthLevel::Maintenance => "maintenance",
        HealthLevel::Degraded => "degraded performance",
        HealthLevel::PartialOutage => "a partial outage",
        HealthLevel::MajorOutage => "a major outage",
        _ => "a problem",
    }
}

pub(super) fn https_url(url: &Url) -> Option<String> {
    (url.scheme() == "https").then(|| url.to_string())
}

pub(super) fn source_title(source: &StatusSource, level: HealthLevel) -> String {
    format!("{} reports {}", source.name, level_label(level))
}

pub(super) fn component_title(component: &ComponentHealth) -> String {
    format!(
        "{}: {}",
        component.name,
        component
            .detail
            .as_deref()
            .unwrap_or_else(|| level_label(component.level))
    )
}

pub(super) fn status_page(
    source: &StatusSource, eval: &FeedSignal, episode_id: &str, view: EvidenceView,
    observed: Observed,
) -> Option<HealthSignal> {
    if eval.level == HealthLevel::Operational
        && eval.incidents.is_empty()
        && eval.maintenances.is_empty()
    {
        return None;
    }
    let title = match (eval.incidents.first(), eval.maintenances.first()) {
        (Some(incident), _) => incident.name.clone(),
        (None, Some(maintenance)) if eval.level <= HealthLevel::Maintenance => {
            maintenance.name.clone()
        }
        _ => source_title(source, eval.level),
    };

    Some(HealthSignal {
        kind: SignalKind::StatusPage,
        episode_id: episode_id.to_owned(),
        level: eval.level,
        evidence: view,
        observed_at: observed.wall,
        title,
        detail: None,
        reported_by: source.name.clone(),
        url: https_url(&source.page_url),
        components: eval.components.clone(),
        incidents: eval.incidents.clone(),
        maintenances: eval.maintenances.clone(),
        updated_at: eval
            .incidents
            .iter()
            .map(|incident| incident.updated_at)
            .max()
            .unwrap_or(observed.wall),
    })
}

pub(super) fn instance(
    display_name: &str, components: &[ComponentHealth], episode_id: &str, view: EvidenceView,
    observed: Observed,
) -> Option<HealthSignal> {
    let mut broken: Vec<ComponentHealth> = components
        .iter()
        .filter(|component| component.level > HealthLevel::Operational)
        .cloned()
        .collect();
    broken.sort_by_key(|component| std::cmp::Reverse(component.level));
    let level = broken.first()?.level;
    let title = match broken.as_slice() {
        [only] => component_title(only),
        many => format!("{} components need attention", many.len()),
    };

    Some(HealthSignal {
        kind: SignalKind::Instance,
        episode_id: episode_id.to_owned(),
        level,
        evidence: view,
        observed_at: observed.wall,
        title,
        detail: None,
        reported_by: display_name.to_owned(),
        url: None,
        components: broken,
        incidents: Vec::new(),
        maintenances: Vec::new(),
        updated_at: observed.wall,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Connection {
    pub kind: SignalKind,
    pub level: HealthLevel,
    pub title: String,
    pub detail: Option<String>,
}

pub(super) fn connection(
    display_name: &str, kind: FailureKind, failures: u32,
) -> Option<Connection> {
    match kind {
        FailureKind::Network | FailureKind::Timeout if failures >= 2 => Some(Connection {
            kind: SignalKind::Connection,
            level: HealthLevel::MajorOutage,
            title: format!("Can't reach {display_name}"),
            detail: Some(CONNECTION_DETAIL.to_owned()),
        }),
        FailureKind::Unavailable if failures >= 2 => Some(Connection {
            kind: SignalKind::Connection,
            level: HealthLevel::PartialOutage,
            title: format!("{display_name} is returning server errors"),
            detail: None,
        }),
        FailureKind::RateLimited => Some(Connection {
            kind: SignalKind::RateLimit,
            level: HealthLevel::Degraded,
            title: format!("Rate limited by {display_name}"),
            detail: None,
        }),
        _ => None,
    }
}

pub(super) fn connection_signal(
    connection: &Connection, episode_id: &str, observed_at: DateTime<Utc>,
) -> HealthSignal {
    HealthSignal {
        kind: connection.kind,
        episode_id: episode_id.to_owned(),
        level: connection.level,
        evidence: EvidenceView::Fresh,
        observed_at,
        title: connection.title.clone(),
        detail: connection.detail.clone(),
        reported_by: PIPEDASH.to_owned(),
        url: None,
        components: Vec::new(),
        incidents: Vec::new(),
        maintenances: Vec::new(),
        updated_at: observed_at,
    }
}

#[cfg(test)]
mod tests {
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
        let signal = |kind, failures| connection("Gitea", kind, failures);

        assert_eq!(signal(FailureKind::Network, 1), None);
        assert_eq!(signal(FailureKind::Timeout, 1), None);
        assert_eq!(signal(FailureKind::Unavailable, 1), None);
        for kind in [FailureKind::Network, FailureKind::Timeout] {
            let major = signal(kind, 2).unwrap();
            assert_eq!(major.level, HealthLevel::MajorOutage);
            assert_eq!(major.kind, SignalKind::Connection);
            assert_eq!(major.title, "Can't reach Gitea");
            assert_eq!(major.detail.as_deref(), Some(CONNECTION_DETAIL));
        }
        let partial = signal(FailureKind::Unavailable, 2).unwrap();
        assert_eq!(partial.level, HealthLevel::PartialOutage);
        assert_eq!(partial.title, "Gitea is returning server errors");
        assert_eq!(partial.detail, None);
        let limited = signal(FailureKind::RateLimited, 1).unwrap();
        assert_eq!(limited.level, HealthLevel::Degraded);
        assert_eq!(limited.kind, SignalKind::RateLimit);
        assert_eq!(limited.title, "Rate limited by Gitea");
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
            "Flux",
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
            "Flux",
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
        assert!(instance("Flux", &[], "e3", EvidenceView::Fresh, now).is_none());
    }
}
