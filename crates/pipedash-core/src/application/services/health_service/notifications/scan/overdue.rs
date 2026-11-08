use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use futures::stream::{
    FuturesUnordered,
    StreamExt,
};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use super::super::{
    LocalNotification,
    NotificationState,
};
use super::{
    Done,
    Overdue,
    Target,
};
use crate::application::services::pipeline_service::overdue::{
    overdue_run,
    tracks_overdue,
};
use crate::application::services::pipeline_service::{
    FETCH_TIMEOUT,
    OVERDUE_CONCURRENT_CHECKS,
    OVERDUE_RECENT_RUNS,
};
use crate::domain::{
    AlertKind,
    DomainError,
    Pipeline,
    PipelineStatus,
    Provider,
};

const OVERDUE_BUDGET: Duration = Duration::from_secs(20);

pub(super) async fn check_overdue(job: Overdue) -> Done {
    let now = Utc::now();
    let provider = &*job.provider;
    let ids: Vec<String> = job
        .selected
        .iter()
        .map(|(pipeline, _)| pipeline.id.clone())
        .collect();
    let checked = match timeout(
        OVERDUE_BUDGET,
        provider.fetch_recent_runs(&ids, OVERDUE_RECENT_RUNS),
    )
    .await
    {
        Ok(Err(DomainError::NotSupported(_))) => check_each(provider, job.selected, now).await,
        Ok(Ok(mut runs)) => job
            .selected
            .into_iter()
            .filter_map(|(pipeline, known)| {
                let overdue = overdue_run(&runs.remove(&pipeline.id)?, now);
                Some((pipeline, known, overdue))
            })
            .collect(),
        Ok(Err(_)) | Err(_) => Vec::new(),
    };

    Done::Overdue {
        id: job.id,
        checked,
    }
}

async fn check_each(
    provider: &dyn Provider, selected: Vec<(Pipeline, bool)>, now: DateTime<Utc>,
) -> Vec<(Pipeline, bool, Option<String>)> {
    let permits = Semaphore::new(OVERDUE_CONCURRENT_CHECKS);
    let mut running: FuturesUnordered<_> = selected
        .into_iter()
        .map(|(pipeline, known)| {
            let permits = &permits;
            async move {
                let _permit = permits.acquire().await.ok()?;
                let runs = timeout(
                    FETCH_TIMEOUT,
                    provider.fetch_run_history(&pipeline.id, OVERDUE_RECENT_RUNS),
                )
                .await
                .ok()?
                .ok()?;
                let overdue = overdue_run(&runs, now);

                Some((pipeline, known, overdue))
            }
        })
        .collect();
    let mut checked = Vec::new();
    let _ = timeout(OVERDUE_BUDGET, async {
        while let Some(result) = running.next().await {
            checked.extend(result);
        }
    })
    .await;
    checked
}

impl NotificationState {
    pub(super) fn overdue_job(
        &self, target: &Target, pipelines: &[Pipeline], limit: usize,
    ) -> Option<Overdue> {
        let marks = self.pipelines.get(&target.id);
        let mark_of = |pipeline: &Pipeline| marks.and_then(|marks| marks.get(&pipeline.id));
        let stamp = |pipeline: &Pipeline| mark_of(pipeline).and_then(|mark| mark.checked_at);
        let policy = target.provider.run_history_policy();
        let mut eligible: Vec<&Pipeline> = pipelines
            .iter()
            .filter(|pipeline| tracks_overdue(pipeline, policy))
            .collect();
        eligible.sort_by(|a, b| stamp(a).cmp(&stamp(b)).then_with(|| a.id.cmp(&b.id)));
        eligible.truncate(limit);
        if eligible.is_empty() {
            return None;
        }
        let selected = eligible
            .into_iter()
            .map(|pipeline| {
                let known = mark_of(pipeline).is_some_and(|mark| {
                    mark.checked_at.is_some() || mark.status != PipelineStatus::Running
                });

                (pipeline.clone(), known)
            })
            .collect();

        Some(Overdue {
            id: target.id,
            provider: Arc::clone(&target.provider),
            selected,
        })
    }

    pub(super) fn settle_overdue(
        &mut self, id: i64, checked: Vec<(Pipeline, bool, Option<String>)>, wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        let mut found = Vec::new();
        for (pipeline, known, overdue) in checked {
            let Some(mark) = self
                .pipelines
                .get_mut(&id)
                .and_then(|marks| marks.get_mut(&pipeline.id))
            else {
                continue;
            };
            mark.checked_at = Some(wall);
            let Some(run) = overdue else {
                continue;
            };
            let changed = mark.overdue.as_deref() != Some(run.as_str());
            mark.overdue = Some(run);
            if known && changed {
                found.push(self.pipeline_notice(
                    &pipeline,
                    format!("{} is running longer than usual", pipeline.name),
                    AlertKind::LongRun,
                ));
            }
        }

        found
    }
}
