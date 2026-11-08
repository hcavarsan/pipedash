use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    TriggerParams,
    TriggerResult,
    WorkflowParameter,
    WorkflowParameterType,
};
use serde_json::{
    json,
    Value,
};
use tracing::{
    info,
    warn,
};

use super::CircleCIPlugin;
use crate::types::{
    PipelineDefinition,
    Project,
    Workflow,
};
use crate::{
    config,
    mapper,
    parameters,
};

fn pick_definition<'a>(
    definitions: &'a [PipelineDefinition], chosen: Option<&str>,
) -> PluginResult<&'a PipelineDefinition> {
    match chosen {
        Some(chosen) => definitions
            .iter()
            .find(|definition| definition.id == chosen || definition.name == chosen)
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("CircleCI has no pipeline named {chosen}"))
            }),
        None => match definitions {
            [only] => Ok(only),
            [] => Err(PluginError::InvalidConfig(
                "This CircleCI project has no pipelines to run".to_string(),
            )),
            _ => Err(PluginError::InvalidConfig(
                "Choose which CircleCI pipeline to run".to_string(),
            )),
        },
    }
}

impl CircleCIPlugin {
    async fn definitions(&self, project: &Project) -> PluginResult<Vec<PipelineDefinition>> {
        let project_id = project.id.as_deref().ok_or_else(|| {
            PluginError::ApiError(format!(
                "CircleCI did not return an id for project {}",
                project.slug
            ))
        })?;
        self.client()?.fetch_definitions(project_id).await
    }

    async fn pipeline_parameters(&self, slug: &str) -> PluginResult<Vec<WorkflowParameter>> {
        let client = self.client()?;
        let Some(latest) = client.fetch_pipelines(slug, 1).await?.into_iter().next() else {
            return Ok(Vec::new());
        };
        let Some(source) = client.fetch_pipeline_config(&latest.id).await? else {
            return Ok(Vec::new());
        };
        match parameters::pipeline_parameters(&source) {
            Ok(declared) => Ok(declared),
            Err(error) => {
                warn!(slug, error = %error, "CircleCI pipeline parameters could not be read");
                Ok(Vec::new())
            }
        }
    }

    pub(super) async fn fetch_workflow_parameters_impl(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let slug = config::parse_pipeline_id(workflow_id)?;
        let project = self.client()?.fetch_project(slug).await?;
        let mut declared = vec![WorkflowParameter {
            name: parameters::BRANCH.to_string(),
            label: Some("Branch".to_string()),
            description: Some("Branch to build".to_string()),
            param_type: WorkflowParameterType::String {
                default: project.default_branch.clone(),
            },
            required: true,
        }];
        if slug.starts_with("circleci/") {
            let definitions = self.definitions(&project).await?;
            if definitions.len() > 1 {
                declared.push(WorkflowParameter {
                    name: parameters::DEFINITION.to_string(),
                    label: Some("Pipeline".to_string()),
                    description: Some("CircleCI pipeline to run".to_string()),
                    param_type: WorkflowParameterType::Choice {
                        options: definitions.iter().map(|d| d.name.clone()).collect(),
                        default: definitions.first().map(|d| d.name.clone()),
                    },
                    required: true,
                });
            }
        }
        declared.extend(self.pipeline_parameters(slug).await?);
        Ok(declared)
    }

    pub(super) async fn trigger_pipeline_impl(
        &self, params: TriggerParams,
    ) -> PluginResult<TriggerResult> {
        let slug = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;
        let inputs = parameters::trigger_inputs(params.inputs);
        let app_project = slug.starts_with("circleci/");
        let project = if app_project || inputs.branch.is_none() {
            Some(client.fetch_project(slug).await?)
        } else {
            None
        };
        let branch = inputs
            .branch
            .or_else(|| project.as_ref().and_then(|p| p.default_branch.clone()))
            .ok_or_else(|| PluginError::InvalidConfig("Choose a branch to build".to_string()))?;

        let number = match project.as_ref().filter(|_| app_project) {
            Some(project) => {
                let definitions = self.definitions(project).await?;
                let definition = pick_definition(&definitions, inputs.definition.as_deref())?;
                let mut body = json!({
                    "definition_id": definition.id,
                    "config": {"branch": branch},
                    "checkout": {"branch": branch},
                });
                if !inputs.parameters.is_empty() {
                    body["parameters"] = Value::Object(inputs.parameters);
                }
                client.run_pipeline(slug, &body).await?
            }
            None => {
                let mut body = json!({ "branch": branch });
                if !inputs.parameters.is_empty() {
                    body["parameters"] = Value::Object(inputs.parameters);
                }
                client.trigger(slug, &body).await?
            }
        };
        info!(slug, number, "Triggered CircleCI pipeline");

        Ok(TriggerResult {
            run_id: None,
            run_number: Some(number),
        })
    }

    pub(super) async fn cancel_run_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<()> {
        let slug = config::parse_pipeline_id(pipeline_id)?;
        let (_, workflows) = self.run_workflows(slug, run_number).await?;
        let active: Vec<&Workflow> = mapper::latest_workflows(&workflows)
            .into_iter()
            .filter(|workflow| mapper::is_active(&workflow.status))
            .collect();
        if active.is_empty() {
            return Err(PluginError::ApiError(format!(
                "Pipeline #{run_number} has no running workflows to cancel"
            )));
        }
        let client = self.client()?;
        for workflow in active {
            client.cancel(&workflow.id).await?;
        }
        info!(slug, run_number, "Cancelled CircleCI pipeline");
        Ok(())
    }

    pub(super) async fn rerun(
        &self, pipeline_id: &str, run_number: i64, failed_only: bool,
    ) -> PluginResult<()> {
        let slug = config::parse_pipeline_id(pipeline_id)?;
        let (_, workflows) = self.run_workflows(slug, run_number).await?;
        let latest = mapper::latest_workflows(&workflows);
        if latest
            .iter()
            .any(|workflow| mapper::is_active(&workflow.status))
        {
            return Err(PluginError::ApiError(format!(
                "Pipeline #{run_number} is still running. Wait for it to finish or cancel it"
            )));
        }
        let chosen: Vec<&Workflow> = latest
            .into_iter()
            .filter(|workflow| {
                !failed_only || matches!(workflow.status.as_str(), "failed" | "error")
            })
            .collect();
        if chosen.is_empty() {
            return Err(PluginError::ApiError(format!(
                "Pipeline #{run_number} has no {}workflows to rerun",
                if failed_only { "failed " } else { "" }
            )));
        }
        let client = self.client()?;
        for workflow in chosen {
            client.rerun(&workflow.id, failed_only).await?;
        }
        info!(slug, run_number, failed_only, "Reran CircleCI pipeline");
        Ok(())
    }
}
