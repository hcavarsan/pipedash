use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    TriggerParams,
    TriggerResult,
    WorkflowParameter,
    WorkflowParameterType,
};
use serde_json::Value;
use tracing::info;

use super::ProwPlugin;
use crate::config;
use crate::deck::RerunMode;
use crate::mapper;

impl ProwPlugin {
    pub(super) async fn fetch_workflow_parameters_impl(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        config::parse_pipeline_id(workflow_id)?;
        let mut parameters = vec![WorkflowParameter {
            name: "prowjob".to_string(),
            label: Some("Run to start again".to_string()),
            description: Some(
                "Name of the ProwJob to start again. Leave empty to use the newest run."
                    .to_string(),
            ),
            param_type: WorkflowParameterType::String { default: None },
            required: false,
        }];
        if self.kube_settings.is_none() {
            parameters.push(WorkflowParameter {
                name: "definition".to_string(),
                label: Some("Job definition".to_string()),
                description: Some(
                    "latest rebuilds the job from Prow's current configuration. original reuses \
                     the exact spec of that run."
                        .to_string(),
                ),
                param_type: WorkflowParameterType::Choice {
                    options: vec!["latest".to_string(), "original".to_string()],
                    default: Some("latest".to_string()),
                },
                required: true,
            });
        }
        Ok(parameters)
    }

    pub(super) async fn trigger_pipeline_impl(
        &self, params: TriggerParams,
    ) -> PluginResult<TriggerResult> {
        self.writable()?;
        let selection = config::parse_pipeline_id(&params.workflow_id)?;
        let job = selection.job.as_str();
        let input = |key: &str| {
            params
                .inputs
                .as_ref()
                .and_then(|inputs| inputs.get(key))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let deck = self.deck()?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;
        let target = match input("prowjob") {
            Some(requested) => jobs
                .iter()
                .find(|candidate| candidate.metadata.name == requested && candidate.spec.job == job)
                .ok_or_else(|| {
                    PluginError::PipelineNotFound(format!(
                        "ProwJob {requested} is not a recent run of {job}"
                    ))
                })?,
            None => mapper::newest(&jobs, job).ok_or_else(|| {
                PluginError::PipelineNotFound(format!(
                    "Deck lists no recent run of {job}, and Prow starts a job again only from an \
                     earlier run"
                ))
            })?,
        };
        let name = target.metadata.name.as_str();

        let created = match self.kube().await? {
            Some(kube) => Some(kube.start_again(target.namespace(), name).await?),
            None => {
                let mode = if input("definition") == Some("original") {
                    RerunMode::Original
                } else {
                    RerunMode::Latest
                };
                deck.rerun(name, mode).await?;
                None
            }
        };
        info!(job, "Prow job started again");

        Ok(TriggerResult {
            run_id: created,
            run_number: None,
        })
    }

    pub(super) async fn cancel_run_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<()> {
        self.writable()?;
        let selection = config::parse_pipeline_id(pipeline_id)?;
        let job = selection.job.as_str();
        let deck = self.deck()?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;
        let target = Self::find_build(&jobs, job, run_number).ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "This build of {job} is no longer listed by Deck"
            ))
        })?;
        if !target.is_active() {
            return Err(PluginError::ApiError(format!(
                "Build {} of {job} already finished ({})",
                target.status.build_id, target.status.state
            )));
        }

        match self.kube().await? {
            Some(kube) => {
                kube.abort(target.namespace(), &target.metadata.name)
                    .await?
            }
            None => deck.abort(&target.metadata.name).await?,
        }
        info!(job, run_number, "Prow job aborted");
        Ok(())
    }
}
