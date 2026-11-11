use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginResult,
    TriggerParams,
    TriggerResult,
    WorkflowParameter,
};

use super::logs::find_run;
use super::TektonPlugin;
use crate::{
    config,
    mapper,
    types,
};

impl TektonPlugin {
    pub(super) async fn fetch_workflow_parameters_impl(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let (namespace, pipeline_name) = config::parse_pipeline_id(workflow_id)?;
        let client = self.client().await?;

        let pipeline = client.get_pipeline(&namespace, &pipeline_name).await?;

        Ok(mapper::map_workflow_parameters(&pipeline))
    }

    pub(super) async fn trigger_pipeline_impl(
        &self, params: TriggerParams,
    ) -> PluginResult<TriggerResult> {
        let (namespace, pipeline_name) = config::parse_pipeline_id(&params.workflow_id)?;

        let client = self.client().await?;

        let pipeline = client.get_pipeline(&namespace, &pipeline_name).await?;

        let param_values: Vec<types::ParamValue> = if let Some(inputs) = &params.inputs {
            inputs
                .as_object()
                .map(|obj| {
                    obj.iter()
                        .map(|(key, value)| types::ParamValue {
                            name: key.clone(),
                            value: value.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        let workspaces: Vec<types::WorkspaceBinding> = pipeline
            .spec
            .workspaces
            .iter()
            .filter_map(|ws| {
                if ws.optional.unwrap_or(false) {
                    None
                } else {
                    Some(types::WorkspaceBinding {
                        name: ws.name.clone(),
                        empty_dir: Some(serde_json::json!({})),
                        persistent_volume_claim: None,
                        config_map: None,
                        secret: None,
                    })
                }
            })
            .collect();

        let run_name = format!("{}-{}", pipeline_name, chrono::Utc::now().timestamp());

        let mut annotations = HashMap::new();
        annotations.insert("tekton.dev/triggeredBy".to_string(), "pipedash".to_string());

        let pipelinerun = types::TektonPipelineRun {
            api_version: "tekton.dev/v1".to_string(),
            kind: "PipelineRun".to_string(),
            metadata: types::ObjectMeta {
                name: run_name.clone(),
                namespace: namespace.clone(),
                uid: None,
                creation_timestamp: None,
                labels: HashMap::new(),
                annotations,
            },
            spec: types::PipelineRunSpec {
                pipeline_ref: Some(types::PipelineRef {
                    name: pipeline_name.clone(),
                }),
                params: param_values,
                workspaces,
                timeout: None,
                task_run_template: None,
            },
            status: types::PipelineRunStatus {
                conditions: vec![],
                start_time: None,
                completion_time: None,
                task_runs: HashMap::new(),
                child_references: vec![],
                results: vec![],
            },
        };

        client.create_pipelinerun(&namespace, &pipelinerun).await?;

        Ok(TriggerResult::default())
    }

    pub(super) async fn cancel_run_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<()> {
        let (namespace, pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client().await?;

        let runs = client
            .list_pipelineruns(&namespace, Some(&pipeline_name))
            .await?;

        let run = find_run(runs, run_number)?;

        client
            .delete_pipelinerun(&namespace, &run.metadata.name)
            .await?;

        Ok(())
    }
}
