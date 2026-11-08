use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    ReviewDecision,
};
use tracing::debug;

use super::{
    map_octocrab_error,
    GitHubClient,
};
use crate::types;

impl GitHubClient {
    pub async fn cancel_run(&self, owner: &str, repo: &str, run_id: u64) -> PluginResult<()> {
        debug!("Cancelling run {run_id} for {owner}/{repo}");

        let url = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/cancel");

        let _: serde_json::Value = self
            .octocrab
            .post(url, None::<&()>)
            .await
            .map_err(|e| map_octocrab_error("Failed to cancel run", e))?;

        debug!("Run {run_id} cancelled successfully");
        Ok(())
    }

    pub async fn force_cancel_run(&self, owner: &str, repo: &str, run_id: u64) -> PluginResult<()> {
        let url = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/force-cancel");
        let _: serde_json::Value = self
            .octocrab
            .post(url, None::<&()>)
            .await
            .map_err(|e| map_octocrab_error("Failed to force cancel run", e))?;

        Ok(())
    }

    pub async fn rerun(
        &self, owner: &str, repo: &str, run_id: u64, failed_only: bool,
    ) -> PluginResult<()> {
        let (endpoint, action) = if failed_only {
            ("rerun-failed-jobs", "Failed to re-run failed jobs")
        } else {
            ("rerun", "Failed to re-run run")
        };
        debug!("POST {endpoint} for run {run_id} of {owner}/{repo}");

        let url = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/{endpoint}");

        let _: serde_json::Value = self
            .octocrab
            .post(url, None::<&()>)
            .await
            .map_err(|e| map_octocrab_error(action, e))?;

        Ok(())
    }

    pub async fn enable_workflow(
        &self, owner: &str, repo: &str, workflow_id: u64,
    ) -> PluginResult<()> {
        debug!("Enabling workflow {workflow_id} for {owner}/{repo}");

        self.octocrab
            .workflows(owner, repo)
            .enable_workflow(workflow_id.to_string())
            .await
            .map_err(|e| map_octocrab_error("Failed to enable workflow", e))
    }

    pub async fn review_pending_deployments(
        &self, owner: &str, repo: &str, run_id: u64, run_number: i64, decision: ReviewDecision,
    ) -> PluginResult<()> {
        let url = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments");

        let pending: Vec<types::PendingDeployment> = self
            .retry_policy
            .retry(|| async {
                self.octocrab
                    .get(&url, None::<&()>)
                    .await
                    .map_err(|e| map_octocrab_error("Failed to fetch pending deployments", e))
            })
            .await?;

        if pending.is_empty() {
            return Err(PluginError::PipelineNotFound(format!(
                "Run #{run_number} isn't waiting for a deployment review"
            )));
        }

        let environment_ids: Vec<u64> = pending
            .iter()
            .filter(|deployment| deployment.current_user_can_approve)
            .map(|deployment| deployment.environment.id)
            .collect();

        if environment_ids.is_empty() {
            let names: Vec<&str> = pending
                .iter()
                .map(|deployment| deployment.environment.name.as_str())
                .collect();
            return Err(PluginError::PermissionDenied(format!(
                "The token's account isn't a required reviewer for {}. Use a token from a required reviewer.",
                names.join(", ")
            )));
        }

        let (state, comment) = match decision {
            ReviewDecision::Approve => ("approved", "Approved in Pipedash"),
            ReviewDecision::Reject => ("rejected", "Rejected in Pipedash"),
        };
        let body = serde_json::json!({
            "environment_ids": environment_ids,
            "state": state,
            "comment": comment,
        });

        let _: serde_json::Value = self
            .octocrab
            .post(&url, Some(&body))
            .await
            .map_err(|e| map_octocrab_error("Failed to review run", e))?;

        Ok(())
    }
}
