use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use tokio::time::Instant;

use super::signals::{
    self,
    Evidence,
};
use super::state::{
    feed_evidence,
    FeedState,
    ProviderEntry,
};
use super::{
    episode_for,
    feed_key,
    FeedKey,
    ProviderHealthState,
};
use crate::domain::{
    EvidenceView,
    HealthLevel,
    HealthSignal,
    SignalKind,
    StatusPageRef,
};
use crate::infrastructure::status_feeds::evaluate;

fn comparable(
    state: &ProviderHealthState,
) -> (HealthLevel, Vec<HealthSignal>, Option<StatusPageRef>) {
    (
        state.level,
        state
            .signals
            .iter()
            .map(|signal| HealthSignal {
                observed_at: DateTime::UNIX_EPOCH,
                updated_at: DateTime::UNIX_EPOCH,
                ..signal.clone()
            })
            .collect(),
        state.status_page.as_ref().map(|page| StatusPageRef {
            checked_at: None,
            ..page.clone()
        }),
    )
}

pub(super) fn same_health(a: &ProviderHealthState, b: &ProviderHealthState) -> bool {
    comparable(a) == comparable(b)
}

pub(super) fn derive(
    entry: &mut ProviderEntry, feeds: &mut HashMap<FeedKey, FeedState>, enabled: bool,
    now: Instant, wall: DateTime<Utc>,
) -> ProviderHealthState {
    let mut signals: Vec<HealthSignal> = Vec::new();
    let mut status_page = None;
    let mut present: Vec<SignalKind> = Vec::new();

    if let Some(source) = &entry.source {
        if let Some(feed) = feeds.get_mut(&feed_key(source)) {
            let evidence = if enabled {
                feed_evidence(feed, now)
            } else {
                Some(Evidence::Suspended)
            };
            if enabled {
                status_page = Some(StatusPageRef {
                    name: source.name.clone(),
                    url: source.page_url.to_string(),
                    checked_at: feed.observed.map(|observed| observed.wall),
                    error: feed.error.map(str::to_owned),
                    components: Vec::new(),
                    other_incidents: Vec::new(),
                });
            }
            if let (Some(parsed), Some((view, observed))) =
                (&feed.last_good, evidence.and_then(Evidence::shown))
            {
                let eval = evaluate(parsed, &source.components, &source.page_url, wall);
                if let Some(page) = status_page.as_mut() {
                    page.components = eval.watched.clone();
                    page.other_incidents = eval.other_incidents.clone();
                }
                if eval.filter_fallback && !feed.logged_fallback {
                    feed.logged_fallback = true;
                    tracing::warn!(
                        source = %source.name,
                        "None of the watched component ids exist in the status feed, watching all components"
                    );
                }
                if let Some(mut signal) = signals::status_page(source, &eval, "", view, observed) {
                    signal.episode_id = episode_for(&mut entry.episodes, SignalKind::StatusPage);
                    present.push(SignalKind::StatusPage);
                    signals.push(signal);
                }
            }
        }
    }

    if entry.has_instance_checks {
        let evidence = Evidence::of(entry.instance.observed, entry.instance.unavailable, now);
        if let Some((view, observed)) = evidence.and_then(Evidence::shown) {
            if let Some(mut signal) = signals::instance(
                &entry.display_name,
                &entry.instance.components,
                "",
                view,
                observed,
            ) {
                signal.episode_id = episode_for(&mut entry.episodes, SignalKind::Instance);
                present.push(SignalKind::Instance);
                signals.push(signal);
            }
        }
    }

    if let Some(outcome) = &entry.outcome {
        if let Some(connection) = outcome
            .kind
            .and_then(|kind| signals::connection(&entry.display_name, kind, outcome.failures))
        {
            let episode = episode_for(&mut entry.episodes, connection.kind);
            present.push(connection.kind);
            signals.push(signals::connection_signal(
                &connection,
                &episode,
                outcome.at,
            ));
        }
    }
    entry.episodes.retain(|kind, _| present.contains(kind));

    signals.sort_by(|a, b| {
        b.level.cmp(&a.level).then_with(|| {
            (a.evidence == EvidenceView::Stale).cmp(&(b.evidence == EvidenceView::Stale))
        })
    });
    let level = signals
        .iter()
        .filter(|signal| signal.evidence == EvidenceView::Fresh)
        .map(|signal| signal.level)
        .max()
        .unwrap_or(HealthLevel::Operational);

    ProviderHealthState {
        generation: entry.generation,
        level,
        signals,
        status_page,
        outage_floor: level == HealthLevel::MajorOutage,
    }
}
