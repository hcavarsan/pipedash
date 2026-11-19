use std::collections::BTreeMap;

use super::{
    LocalNotification,
    NotificationState,
    PipelineMark,
};
use crate::domain::pipeline::notice_title;
use crate::domain::{
    AlertKind,
    Pipeline,
    PipelineStatus,
    RunFinish,
};

impl NotificationState {
    pub(super) fn observe_pipelines(
        &mut self, provider_id: i64, pipelines: &[Pipeline],
    ) -> Vec<LocalNotification> {
        let previous = self.pipelines.remove(&provider_id).unwrap_or_default();
        let mut next = BTreeMap::new();
        let mut found = Vec::new();
        for pipeline in pipelines {
            let old = previous.get(&pipeline.id);
            let finish = old.and_then(|old| old.finish_of(pipeline));
            let mark = next_mark(pipeline, old, finish);
            if let Some(old) = old {
                let awaiting_success = old.awaiting_success || old.status == PipelineStatus::Failed;
                let recovered = awaiting_success && pipeline.status == PipelineStatus::Success;
                let needs_attention = pipeline.status.needs_attention()
                    && (old.status != pipeline.status || old.last_run != pipeline.last_run);
                if needs_attention {
                    found.push(LocalNotification {
                        finish,
                        ..self.pipeline_notice(
                            pipeline,
                            attention_event(pipeline),
                            attention_kind(pipeline),
                        )
                    });
                } else if recovered {
                    found.push(LocalNotification {
                        finish,
                        ..self.pipeline_notice(pipeline, "passed again", AlertKind::Recovered)
                    });
                } else if let Some(finish) = finish {
                    found.push(LocalNotification {
                        finish: Some(finish),
                        ..self.pipeline_notice(
                            pipeline,
                            finished_event(pipeline),
                            AlertKind::Finished,
                        )
                    });
                }
                if pipeline.overdue_run.is_some() && old.overdue != pipeline.overdue_run {
                    found.push(self.pipeline_notice(
                        pipeline,
                        "is running longer than usual",
                        AlertKind::LongRun,
                    ));
                }
                if let Some(cadence) = pipeline
                    .cadence
                    .filter(|cadence| cadence.quiet && !old.quiet)
                {
                    found.push(self.quiet_notice(pipeline, cadence.usual_gap_seconds));
                }
            }
            next.insert(pipeline.id.clone(), mark);
        }
        if !next.is_empty() {
            self.pipelines.insert(provider_id, next);
        }

        found
    }

    pub(super) fn observe_failed(&mut self, pipelines: &[Pipeline]) -> Vec<LocalNotification> {
        let mut found = Vec::new();
        for pipeline in pipelines
            .iter()
            .filter(|pipeline| pipeline.status.needs_attention())
        {
            let marks = self.pipelines.entry(pipeline.provider_id).or_default();
            let old = marks.get(&pipeline.id);
            let handled = old.is_some_and(|old| {
                old.status == pipeline.status && old.last_run == pipeline.last_run
            });
            let finish = old.and_then(|old| old.finish_of(pipeline));
            let mark = next_mark(pipeline, old, finish);
            marks.insert(pipeline.id.clone(), mark);
            if !handled {
                found.push(LocalNotification {
                    finish,
                    ..self.pipeline_notice(
                        pipeline,
                        attention_event(pipeline),
                        attention_kind(pipeline),
                    )
                });
            }
        }

        found
    }

    pub(super) fn pipeline_notice(
        &self, pipeline: &Pipeline, event: &str, kind: AlertKind,
    ) -> LocalNotification {
        let provider = self
            .providers
            .get(&pipeline.provider_id)
            .map(|mark| mark.name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&pipeline.provider_type);
        let message = pipeline
            .resource_state
            .as_ref()
            .and_then(|state| state.message.as_deref())
            .unwrap_or_default();
        let body = [provider, message]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");

        LocalNotification {
            key: format!("pipeline:{}", pipeline.id),
            title: notice_title(&pipeline.name, event, &pipeline.repository),
            body,
            pipeline_id: Some(pipeline.id.clone()),
            provider_id: Some(pipeline.provider_id),
            provider_ids: vec![pipeline.provider_id],
            kind,
            finish: None,
            level: None,
            link: None,
        }
    }
}

fn attention_event(pipeline: &Pipeline) -> &'static str {
    match pipeline.status {
        PipelineStatus::Degraded => "is degraded",
        _ => "failed",
    }
}

fn finished_event(pipeline: &Pipeline) -> &'static str {
    match pipeline.status {
        PipelineStatus::Cancelled => "was cancelled",
        PipelineStatus::Skipped => "was skipped",
        _ => "succeeded",
    }
}

fn attention_kind(pipeline: &Pipeline) -> AlertKind {
    match pipeline.status {
        PipelineStatus::Degraded => AlertKind::Degraded,
        _ => AlertKind::Failed,
    }
}

impl NotificationState {
    fn quiet_notice(&self, pipeline: &Pipeline, usual_gap_seconds: i64) -> LocalNotification {
        let mut notice = self.pipeline_notice(pipeline, "stopped running", AlertKind::Quiet);
        let usual = format!("Usually runs {}", every(usual_gap_seconds));
        notice.body = if notice.body.is_empty() {
            usual
        } else {
            format!("{} · {usual}", notice.body)
        };

        notice
    }
}

fn every(seconds: i64) -> String {
    let (count, unit) = match seconds {
        s if s >= 86_400 => ((s + 43_200) / 86_400, "day"),
        s if s >= 3_600 => ((s + 1_800) / 3_600, "hour"),
        s => (((s + 30) / 60).max(1), "minute"),
    };
    if count == 1 {
        format!("every {unit}")
    } else {
        format!("every {count} {unit}s")
    }
}

fn next_mark(
    pipeline: &Pipeline, previous: Option<&PipelineMark>, finish: Option<RunFinish>,
) -> PipelineMark {
    let mut mark = PipelineMark::of(pipeline);
    let Some(old) = previous else {
        return mark;
    };
    mark.finished_at = finish.map(|finish| finish.at).or(old.finished_at);
    if pipeline.status != PipelineStatus::Success {
        mark.awaiting_success |= old.awaiting_success || old.status == PipelineStatus::Failed;
    }
    if mark.overdue.is_none() {
        mark.overdue = old.overdue.clone();
    }
    mark.checked_at = old.checked_at;
    if pipeline.cadence.is_none() && pipeline.last_run == old.last_run {
        mark.quiet = old.quiet;
    }

    mark
}
