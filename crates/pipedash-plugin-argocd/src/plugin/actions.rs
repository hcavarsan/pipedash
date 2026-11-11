use pipedash_plugin_api::{
    PluginResult,
    TriggerParams,
    TriggerResult,
    WorkflowParameter,
    WorkflowParameterType,
};
use tracing::info;

use super::ArgocdPlugin;
use crate::types::SyncRequest;
use crate::{
    client,
    config,
};

impl ArgocdPlugin {
    pub(super) async fn trigger_pipeline_impl(
        &self, params: TriggerParams,
    ) -> PluginResult<TriggerResult> {
        let (namespace, app_name) = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;

        let inputs = params.inputs.as_ref();

        let revision = inputs
            .and_then(|i| i.get("revision"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let prune = inputs
            .and_then(|i| i.get("prune"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let dry_run = inputs
            .and_then(|i| i.get("dry_run"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let force = inputs
            .and_then(|i| i.get("force"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let apply_only = inputs
            .and_then(|i| i.get("apply_only"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        info!(
            app_name,
            ?revision,
            prune,
            dry_run,
            force,
            apply_only,
            "Triggering ArgoCD sync operation"
        );

        let request = SyncRequest {
            revision,
            prune: Some(prune),
            dry_run: Some(dry_run),
            strategy: client::sync_strategy(force, apply_only),
        };
        client
            .sync_application(&namespace, &app_name, &request)
            .await?;

        Ok(TriggerResult::default())
    }

    pub(super) async fn fetch_workflow_parameters_impl(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![
            WorkflowParameter {
                name: "revision".to_string(),
                label: Some("Revision".to_string()),
                description: Some("Git revision (branch, tag, or commit SHA) to sync to. Leave empty to use target revision.".to_string()),
                param_type: WorkflowParameterType::String { default: None },
                required: false,
            },
            WorkflowParameter {
                name: "prune".to_string(),
                label: Some("Prune Resources".to_string()),
                description: Some("Delete resources that are no longer defined in Git".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
            WorkflowParameter {
                name: "dry_run".to_string(),
                label: Some("Dry Run".to_string()),
                description: Some("Preview sync without applying changes".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
            WorkflowParameter {
                name: "force".to_string(),
                label: Some("Force Sync".to_string(),),
                description: Some("Force sync even if resources are already synced (overrides any state)".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
            WorkflowParameter {
                name: "apply_only".to_string(),
                label: Some("Apply Only (Skip Hooks)".to_string()),
                description: Some("Skip pre and post sync hooks".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
        ])
    }
}
