use crate::domain::provider_attention::{
    AttentionSchedule,
    AttentionTone,
    ProviderAttention,
    ProviderOpenAction,
};
use crate::domain::{
    EvidenceView,
    FailureKind,
    FetchStatus,
    FreshnessState,
    HealthLevel,
    HealthSignal,
    ProviderRecovery,
    ProviderSummary,
    SignalKind,
};

const UNREACHABLE_HINT: &str =
    "Pipedash can't reach this provider. Check the network or VPN of the machine running Pipedash.";
const SIGN_IN_HINT: &str = "Your sign-in expired. Sign in again to reconnect.";
const SETTINGS_HINT: &str = "Check the provider URL and certificate settings.";

struct Lead {
    tone: AttentionTone,
    headline: String,
    detail: Option<String>,
    episode_key: String,
    explains_failure: bool,
}

fn is_outage_level(level: HealthLevel) -> bool {
    matches!(level, HealthLevel::PartialOutage | HealthLevel::MajorOutage)
}

fn kind_hint(kind: FailureKind) -> Option<&'static str> {
    match kind {
        FailureKind::Auth => Some("The token was rejected. Update it in Settings → Providers."),
        FailureKind::Permission => {
            Some("The token is missing permissions. Check them in Settings → Providers.")
        }
        FailureKind::Config | FailureKind::Tls => Some(SETTINGS_HINT),
        FailureKind::Network | FailureKind::Timeout => Some(UNREACHABLE_HINT),
        FailureKind::RateLimited => Some("Rate limited."),
        FailureKind::Unavailable => Some("The provider is returning server errors."),
        FailureKind::NotFound | FailureKind::Other => None,
    }
}

fn failed(summary: &ProviderSummary) -> bool {
    matches!(summary.last_fetch_status, FetchStatus::Error)
}

fn fetch_hint(summary: &ProviderSummary) -> Option<String> {
    if !failed(summary) {
        return None;
    }
    if summary.recovery == Some(ProviderRecovery::SignIn) {
        return Some(SIGN_IN_HINT.to_owned());
    }
    summary
        .last_fetch_error_kind
        .and_then(kind_hint)
        .map(str::to_owned)
        .or_else(|| summary.last_fetch_error.clone())
}

fn pick_lead(
    summary: &ProviderSummary, first_fresh: Option<&HealthSignal>, kind: Option<FailureKind>,
) -> Option<Lead> {
    if let Some(signal) = first_fresh.filter(|_| is_outage_level(summary.health.level)) {
        return Some(Lead {
            tone: AttentionTone::Warning,
            headline: signal.title.clone(),
            detail: signal.detail.clone(),
            episode_key: format!("{}:{}", summary.id, signal.episode_id),
            explains_failure: signal.kind == SignalKind::Connection,
        });
    }
    if failed(summary) && !kind.is_some_and(FailureKind::is_transient) {
        let label = serde_json::to_value(kind.unwrap_or(FailureKind::Other))
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "other".to_owned());
        return Some(Lead {
            tone: AttentionTone::Danger,
            headline: "Fetch failed".to_owned(),
            detail: summary.last_fetch_error.clone(),
            episode_key: format!("{}:fetch-failed:{label}", summary.id),
            explains_failure: false,
        });
    }
    None
}

fn open_action(summary: &ProviderSummary) -> ProviderOpenAction {
    if summary.recovery == Some(ProviderRecovery::SignIn) {
        ProviderOpenAction::SignIn
    } else if failed(summary)
        || summary.freshness.state == FreshnessState::Down
        || is_outage_level(summary.health.level)
    {
        ProviderOpenAction::Status
    } else {
        ProviderOpenAction::Pipelines
    }
}

pub(crate) fn provider_attention(summary: &ProviderSummary) -> ProviderAttention {
    let health = &summary.health;
    let kind = failed(summary)
        .then_some(summary.last_fetch_error_kind)
        .flatten();
    let is_fresh = |signal: &&HealthSignal| signal.evidence == EvidenceView::Fresh;
    let lead = pick_lead(summary, health.signals.iter().find(is_fresh), kind);
    let schedule = lead.as_ref().and_then(|_| {
        if health.in_flight {
            Some(AttentionSchedule::CheckingNow)
        } else {
            health
                .next_attempt_at
                .map(|at| AttentionSchedule::NextAttempt { at })
        }
    });
    let fetch_hint = fetch_hint(summary);
    let hint = lead
        .as_ref()
        .filter(|lead| !lead.explains_failure)
        .and(fetch_hint.clone());
    let (tone, headline, detail, episode_key) = match lead {
        Some(lead) => (
            Some(lead.tone),
            Some(lead.headline),
            lead.detail,
            Some(lead.episode_key),
        ),
        None => (None, None, None, None),
    };

    ProviderAttention {
        tone,
        headline,
        detail,
        hint,
        fetch_hint,
        schedule,
        kind,
        recovery: summary.recovery,
        episode_key,
        refreshing_normally: matches!(summary.last_fetch_status, FetchStatus::Success)
            && !health
                .signals
                .iter()
                .any(|signal| signal.kind == SignalKind::Connection)
            && health.next_attempt_at.is_none(),
        stale_signals: health
            .signals
            .iter()
            .filter(|signal| signal.evidence == EvidenceView::Stale)
            .cloned()
            .collect(),
        scheduled_maintenances: health
            .signals
            .iter()
            .filter(is_fresh)
            .flat_map(|signal| &signal.maintenances)
            .filter(|item| !item.in_progress && item.starts_at.is_some())
            .cloned()
            .collect(),
        open_action: open_action(summary),
    }
}

#[cfg(test)]
mod tests;
