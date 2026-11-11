use std::collections::BTreeSet;

use chrono::{
    DateTime,
    Duration,
    Utc,
};
use pipedash_plugin_api::Correlation;

use super::{
    CorrelationService,
    OBSERVATION_GAP_FACTOR,
    OBSERVATION_GAP_FLOOR_SECS,
};
use crate::domain::{
    DomainResult,
    Pipeline,
};
use crate::infrastructure::database::{
    CorrelationConn,
    ObservationWindowRow,
    ObservedTransitionRow,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Snapshot {
    pub revision: Option<String>,
    pub digests: Vec<String>,
}

impl Snapshot {
    fn of(correlation: &Correlation) -> Self {
        let digests: BTreeSet<String> = correlation
            .consumed
            .iter()
            .filter_map(|c| c.artifact.digest.as_ref())
            .map(|d| d.as_str().to_string())
            .collect();
        Self {
            revision: correlation
                .sources
                .first()
                .and_then(|s| s.commit.as_ref())
                .map(|c| c.as_str().to_string()),
            digests: digests.into_iter().collect(),
        }
    }
}

pub(super) fn health_of(pipeline: &Pipeline) -> Option<String> {
    pipeline.resource_state.as_ref().map(|s| s.label.clone())
}

impl CorrelationService {
    pub(super) async fn touch_window(
        &self, conn: &mut CorrelationConn, provider_id: i64, refresh_interval: i64,
        now: DateTime<Utc>,
    ) -> DomainResult<()> {
        let gap = Duration::seconds(
            refresh_interval
                .saturating_mul(OBSERVATION_GAP_FACTOR)
                .max(OBSERVATION_GAP_FLOOR_SECS),
        );
        let windows = conn
            .list_observation_windows_by_provider(provider_id)
            .await?;
        let started_at = match windows.last() {
            Some(last) if now - last.ended_at <= gap => last.started_at,
            _ => now,
        };
        conn.upsert_observation_window(&ObservationWindowRow {
            provider_id,
            started_at,
            ended_at: now,
        })
        .await
    }

    pub(super) async fn record_transition(
        &self, conn: &mut CorrelationConn, provider_id: i64, now: DateTime<Utc>,
        old: Option<&Pipeline>, new: &Pipeline, before: &Snapshot,
    ) -> DomainResult<()> {
        let after = Snapshot::of(&new.correlation);
        let health_before = old.and_then(health_of);
        let health_after = health_of(new);
        let steady = old.is_some_and(|o| o.status == new.status)
            && health_before == health_after
            && *before == after;
        if steady {
            return Ok(());
        }
        let row = ObservedTransitionRow {
            id: 0,
            pipeline_id: new.id.clone(),
            provider_id,
            at: now,
            status_before: old.map(|o| o.status.as_str().to_string()),
            status_after: new.status.as_str().to_string(),
            health_before,
            health_after,
            revision: after.revision,
            digests_json: serde_json::to_string(&after.digests).unwrap_or_default(),
        };
        conn.insert_observed_transition(&row).await?;
        Ok(())
    }
}
