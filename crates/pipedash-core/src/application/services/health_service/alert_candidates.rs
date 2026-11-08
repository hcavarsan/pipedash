use pipedash_plugin_api::{
    ComponentHealth,
    StatusSource,
};

use super::alerts::{
    Candidate,
    Draft,
    Observation,
    CALM_CHECKS,
};
use super::signals::{
    component_title,
    connection,
    https_url,
    level_label,
    source_title,
    Connection,
};
use crate::domain::{
    FailureKind,
    HealthLevel,
    HealthSignal,
    SignalKind,
};
use crate::event::{
    AlertLink,
    AlertLinkKind,
};
use crate::infrastructure::status_feeds::FeedSignal;

const CONNECTION_KEY: &str = "connection";
const SOURCE_KEY: &str = "source";

fn page_link(source: &StatusSource) -> Option<AlertLink> {
    https_url(&source.page_url).map(|url| AlertLink {
        url,
        kind: AlertLinkKind::StatusPage,
    })
}

pub(super) fn feed_candidates(
    source: &StatusSource, eval: &FeedSignal, provider_id: i64,
) -> Vec<Candidate> {
    let page = page_link(source);
    let mut candidates: Vec<Candidate> = eval
        .incidents
        .iter()
        .filter(|incident| incident.level >= HealthLevel::PartialOutage)
        .map(|incident| Candidate {
            key: format!("incident:{}", incident.id),
            draft: Draft {
                level: incident.level,
                title: incident.name.clone(),
                body: incident
                    .latest_update
                    .clone()
                    .unwrap_or_else(|| incident.phase.clone()),
                link: match &incident.url {
                    Some(url) if page.as_ref().is_none_or(|page| page.url != *url) => {
                        Some(AlertLink {
                            url: url.clone(),
                            kind: AlertLinkKind::Incident,
                        })
                    }
                    _ => page.clone(),
                },
            },
            provider_ids: vec![provider_id],
        })
        .collect();

    if candidates.is_empty() && eval.level >= HealthLevel::PartialOutage {
        candidates.push(Candidate {
            key: SOURCE_KEY.to_owned(),
            draft: Draft {
                level: eval.level,
                title: source_title(source, eval.level),
                body: eval
                    .components
                    .iter()
                    .take(3)
                    .map(|component| {
                        format!("{}: {}", component.name, level_label(component.level))
                    })
                    .collect::<Vec<_>>()
                    .join("; "),
                link: page,
            },
            provider_ids: vec![provider_id],
        });
    }

    candidates
}

pub(super) fn seeded_keys(signal: &HealthSignal) -> Vec<String> {
    match signal.kind {
        SignalKind::StatusPage => {
            let mut keys: Vec<String> = signal
                .incidents
                .iter()
                .filter(|incident| incident.level >= HealthLevel::PartialOutage)
                .map(|incident| format!("incident:{}", incident.id))
                .collect();
            if keys.is_empty() && signal.level >= HealthLevel::PartialOutage {
                keys.push(SOURCE_KEY.to_owned());
            }
            keys
        }
        SignalKind::Instance => signal
            .components
            .iter()
            .filter(|component| component.level == HealthLevel::MajorOutage)
            .map(|component| format!("component:{}", component.name))
            .collect(),
        SignalKind::Connection => vec![CONNECTION_KEY.to_owned()],
        SignalKind::RateLimit => Vec::new(),
    }
}

pub(super) fn feed_recovery(source: &StatusSource) -> Draft {
    Draft {
        level: HealthLevel::Operational,
        title: format!("{} is back to normal", source.name),
        body: "The status page no longer reports an outage.".to_owned(),
        link: page_link(source),
    }
}

pub(super) fn instance_candidates(
    display_name: &str, components: &[ComponentHealth], provider_id: i64,
) -> Vec<Candidate> {
    components
        .iter()
        .filter(|component| component.level == HealthLevel::MajorOutage)
        .map(|component| Candidate {
            key: format!("component:{}", component.name),
            draft: Draft {
                level: component.level,
                title: component_title(component),
                body: format!("Reported by the controller health check of {display_name}."),
                link: None,
            },
            provider_ids: vec![provider_id],
        })
        .collect()
}

pub(super) fn instance_recovery(display_name: &str) -> Draft {
    Draft {
        level: HealthLevel::Operational,
        title: format!("{display_name} controllers recovered"),
        body: "The controller health check no longer reports an outage.".to_owned(),
        link: None,
    }
}

pub(super) fn connection_candidate(connection: &Connection, provider_id: i64) -> Candidate {
    Candidate {
        key: CONNECTION_KEY.to_owned(),
        draft: Draft {
            level: connection.level,
            title: connection.title.clone(),
            body: connection
                .detail
                .clone()
                .unwrap_or_else(|| "Pipedash will keep retrying with a growing delay.".to_owned()),
            link: None,
        },
        provider_ids: vec![provider_id],
    }
}

pub(super) fn connection_recovery(display_name: &str) -> Draft {
    Draft {
        level: HealthLevel::Operational,
        title: format!("{display_name} is reachable again"),
        body: "Pipedash's connection to it works again.".to_owned(),
        link: None,
    }
}

pub(super) enum Connectivity {
    Reset,
    Ignore,
    Observe(Vec<Candidate>),
}

pub(super) fn connectivity(
    display_name: &str, kind: Option<FailureKind>, failures: u32, provider_id: i64,
) -> Connectivity {
    match kind {
        None => Connectivity::Observe(Vec::new()),
        Some(kind) if !kind.is_transient() => Connectivity::Reset,
        Some(kind) => match connection(display_name, kind, failures) {
            Some(current) if current.kind == SignalKind::Connection => {
                Connectivity::Observe(vec![connection_candidate(&current, provider_id)])
            }
            _ => Connectivity::Ignore,
        },
    }
}

pub(super) fn merge(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut merged: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        match merged.iter_mut().find(|known| known.key == candidate.key) {
            Some(known) => {
                known.provider_ids.extend(candidate.provider_ids);
                known.draft.level = known.draft.level.max(candidate.draft.level);
            }
            None => merged.push(candidate),
        }
    }
    for candidate in &mut merged {
        candidate.provider_ids.sort_unstable();
        candidate.provider_ids.dedup();
    }

    merged
}

pub(super) fn feed_observation(
    candidates: Vec<Candidate>, recovery: Option<Draft>, provider_ids: Vec<i64>,
) -> Option<Observation> {
    Some(Observation {
        candidates: merge(candidates),
        recovery: recovery?,
        provider_ids,
        calm_needed: CALM_CHECKS,
    })
}
