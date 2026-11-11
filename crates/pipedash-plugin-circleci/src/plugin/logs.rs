use bytes::Bytes;
use futures::{
    stream,
    StreamExt,
};
use pipedash_plugin_api::logs::rendered::RenderedCursor;
use pipedash_plugin_api::support::{
    numeric_id,
    run_metadata_str,
    try_buffered,
};
use pipedash_plugin_api::{
    LogCursor,
    LogNode,
    LogRead,
    LogSink,
    PluginResult,
    RunRef,
};

use super::{
    CircleCIPlugin,
    CONCURRENCY,
};
use crate::log_dialect::{
    self,
    Section,
};
use crate::{
    config,
    mapper,
};

const FAILED_STEP_STATUSES: [&str; 4] = ["failed", "timedout", "infrastructure_fail", "canceled"];

impl CircleCIPlugin {
    async fn job_sections(&self, slug: &str, job_number: i64) -> PluginResult<Vec<u8>> {
        let client = self.client()?;
        let steps = client.fetch_job_steps(slug, job_number).await?;
        let actions: Vec<_> = steps
            .steps
            .iter()
            .flat_map(|step| {
                let parallel = step.actions.len() > 1;
                step.actions
                    .iter()
                    .map(move |action| (step, action, parallel))
            })
            .collect();
        let requests: Vec<_> = actions
            .iter()
            .map(|(_, action, _)| async move {
                match action.output_url.as_deref().filter(|url| !url.is_empty()) {
                    Some(url) => client.fetch_step_output(url, self.step_output_limit).await,
                    None => Ok(Some(Vec::new())),
                }
            })
            .collect();
        let mut downloads = stream::iter(requests).buffered(CONCURRENCY);
        let mut outputs = Vec::with_capacity(actions.len());
        let mut total = 0usize;
        while let Some(output) = downloads.next().await {
            let output = output?;
            total += output
                .iter()
                .flatten()
                .map(|message| message.message.len())
                .sum::<usize>();
            if total > self.job_output_limit {
                break;
            }
            outputs.push(output);
        }
        drop(downloads);
        outputs.resize_with(actions.len(), || None);
        let sections: Vec<Section<'_>> = actions
            .iter()
            .zip(&outputs)
            .map(|((step, action, parallel), output)| Section {
                name: if *parallel {
                    format!("{} (container {})", step.name, action.index)
                } else {
                    step.name.clone()
                },
                failed: action.failed == Some(true)
                    || action
                        .status
                        .as_deref()
                        .is_some_and(|status| FAILED_STEP_STATUSES.contains(&status)),
                started_at: action.start_time,
                output: output.as_deref(),
            })
            .collect();
        Ok(log_dialect::render(&sections))
    }

    pub(super) async fn fetch_log_nodes_impl(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let slug = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let pipeline_uuid = match run_metadata_str(&run, "circleci_pipeline_id") {
            Some(id) => id.to_string(),
            None => client.fetch_pipeline(slug, run.run_number).await?.id,
        };
        let workflows = client.fetch_workflows(&pipeline_uuid).await?;
        let latest = mapper::latest_workflows(&workflows);
        let requests: Vec<_> = latest
            .iter()
            .map(|workflow| client.fetch_jobs(&workflow.id))
            .collect();
        let jobs: Vec<_> = try_buffered(requests, CONCURRENCY).await?;
        let grouped: Vec<_> = latest.into_iter().zip(jobs).collect();
        Ok(mapper::log_nodes(
            &self.base_url,
            slug,
            run.run_number,
            &grouped,
        ))
    }

    pub(super) async fn read_log_impl(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let job_number = numeric_id(node_id, "log node ID")?;
        let slug = config::parse_pipeline_id(run.pipeline_id)?;
        let rendered = Bytes::from(self.job_sections(slug, job_number).await?);
        RenderedCursor::read(rendered, from, sink).await
    }
}
