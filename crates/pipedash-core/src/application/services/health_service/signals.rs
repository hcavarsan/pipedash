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
mod tests;
