use pipedash_plugin_actions::dispatch_body;
use pipedash_plugin_api::*;

use super::GitHubPlugin;
use crate::client;

impl GitHubPlugin {
    fn split_id(pipeline_id: &str) -> PluginResult<(&str, &str, &str)> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        let [owner, repo, workflow] = parts.as_slice() else {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: '{pipeline_id}'. Expected format: '{{owner}}__{{repo}}__{{workflow_id}}'"
            )));
        };

        Ok((owner, repo, workflow))
    }

    pub(super) fn repo_of(pipeline_id: &str) -> PluginResult<(&str, &str)> {
        let (owner, repo, _) = Self::split_id(pipeline_id)?;

        Ok((owner, repo))
    }

    pub(super) fn pipeline_ref(pipeline_id: &str) -> PluginResult<(&str, &str, u64)> {
        let (owner, repo, workflow) = Self::split_id(pipeline_id)?;
        let workflow_id: u64 = workflow
            .parse()
            .map_err(|_| PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow}")))?;

        Ok((owner, repo, workflow_id))
    }

    pub(super) fn matches_trigger(
        run: &crate::types::Run, previous_run_number: i64, ref_value: &str,
        trigger_time: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        run.run_number > previous_run_number
            && run.head_branch == ref_value
            && run.created_at.timestamp() >= trigger_time.timestamp()
    }

    async fn run_of<'a>(
        &self, pipeline_id: &'a str, run_number: i64,
    ) -> PluginResult<(&client::GitHubClient, &'a str, &'a str, u64)> {
        let (owner, repo, workflow_id) = Self::pipeline_ref(pipeline_id)?;
        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        Ok((client, owner, repo, run.id.0))
    }

    pub(super) async fn rerun(
        &self, pipeline_id: &str, run_number: i64, failed_only: bool,
    ) -> PluginResult<()> {
        let (client, owner, repo, run_id) = self.run_of(pipeline_id, run_number).await?;
        client.rerun(owner, repo, run_id, failed_only).await
    }

    pub(super) async fn trigger_pipeline_impl(
        &self, params: TriggerParams,
    ) -> PluginResult<TriggerResult> {
        let (owner, repo, workflow_id_u64) = Self::pipeline_ref(&params.workflow_id)?;
        let workflow_id = workflow_id_u64.to_string();

        let mut body = dispatch_body(params.inputs);
        let ref_value = body["ref"].as_str().unwrap_or("main").to_string();
        let client = self.client()?;
        let actions = client.octocrab.actions();
        let mut dispatch = actions.create_workflow_dispatch(owner, repo, &workflow_id, &ref_value);
        if let Some(inputs) = body.get_mut("inputs") {
            dispatch = dispatch.inputs(inputs.take());
        }

        let previous_runs = client
            .fetch_run_history(owner, repo, workflow_id_u64, 5)
            .await?;
        let previous_latest_run_number = previous_runs.first().map(|r| r.run_number).unwrap_or(0);
        let trigger_time = chrono::Utc::now();

        dispatch
            .send()
            .await
            .map_err(|e| client::map_octocrab_error("Failed to trigger workflow", e))?;

        let mut new_run = None;
        for attempt in 1..=10 {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

            let runs = client
                .fetch_run_history(owner, repo, workflow_id_u64, 5)
                .await?;

            new_run = runs.into_iter().find(|run| {
                Self::matches_trigger(run, previous_latest_run_number, &ref_value, trigger_time)
            });

            if let Some(run) = &new_run {
                tracing::debug!(
                    run_number = run.run_number,
                    attempt = attempt,
                    "Found new GitHub run"
                );
                break;
            }

            tracing::debug!(
                attempt = attempt,
                max_attempts = 10,
                "Waiting for new GitHub run to appear"
            );
        }

        let run_number = match new_run {
            Some(run) => run.run_number,
            None => client
                .fetch_run_history(owner, repo, workflow_id_u64, 1)
                .await?
                .first()
                .map_or(0, |run| run.run_number),
        };

        Ok(TriggerResult {
            run_id: None,
            run_number: (run_number > 0).then_some(run_number),
        })
    }

    pub(super) async fn cancel_run_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<()> {
        let (client, owner, repo, run_id) = self.run_of(pipeline_id, run_number).await?;
        client.cancel_run(owner, repo, run_id).await
    }

    pub(super) async fn force_cancel_run_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<()> {
        let (client, owner, repo, run_id) = self.run_of(pipeline_id, run_number).await?;
        client.force_cancel_run(owner, repo, run_id).await
    }

    pub(super) async fn review_run_impl(
        &self, pipeline_id: &str, run_number: i64, decision: ReviewDecision,
    ) -> PluginResult<()> {
        let (client, owner, repo, run_id) = self.run_of(pipeline_id, run_number).await?;
        client
            .review_pending_deployments(owner, repo, run_id, run_number, decision)
            .await
    }

    pub(super) async fn enable_pipeline_impl(&self, pipeline_id: &str) -> PluginResult<()> {
        let (owner, repo, workflow_id) = Self::pipeline_ref(pipeline_id)?;

        self.client()?
            .enable_workflow(owner, repo, workflow_id)
            .await
    }
}
