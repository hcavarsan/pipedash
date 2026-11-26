use std::collections::HashMap;
use std::sync::{
    Mutex,
    MutexGuard,
    PoisonError,
};

use octocrab::models::workflows::WorkFlow;
use octocrab::models::WorkflowId;
use pipedash_plugin_api::{
    support::join_buffered,
    ClusterId,
    Digest,
    PathScope,
    PluginResult,
    TargetObservation,
};
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;

use super::{
    map_octocrab_error,
    GitHubClient,
};
use crate::correlation::{
    self,
    paths,
};
use crate::{
    config,
    types,
};

const FETCH_CONCURRENCY: usize = 8;
const PER_PAGE: u32 = 100;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl GitHubClient {
    pub(crate) fn token(&self) -> &str {
        self.token.expose_secret()
    }

    async fn fetch<T: DeserializeOwned>(
        &self, context: &str, route: &str, query: Option<serde_json::Value>,
    ) -> PluginResult<T> {
        self.retry_policy
            .retry(|| async {
                self.octocrab
                    .get::<T, _, _>(route, query.as_ref())
                    .await
                    .map_err(|e| map_octocrab_error(context, e))
            })
            .await
    }

    pub(crate) async fn fetch_commit(
        &self, owner: &str, repo: &str, rev: &str,
    ) -> PluginResult<types::CommitResponse> {
        let route = format!("/repos/{owner}/{repo}/commits/{rev}");
        self.fetch("Failed to fetch commit", &route, None).await
    }

    pub(crate) async fn compare_commits(
        &self, owner: &str, repo: &str, base: &str, head: &str,
    ) -> PluginResult<types::Comparison> {
        let route = format!("/repos/{owner}/{repo}/compare/{base}...{head}");
        let query = serde_json::json!({"per_page": PER_PAGE});
        self.fetch("Failed to compare commits", &route, Some(query))
            .await
    }

    pub(crate) async fn attestation_payloads(
        &self, owner: &str, repo: &str, digest: &Digest,
    ) -> PluginResult<Vec<String>> {
        let route = format!("/repos/{owner}/{repo}/attestations/{}", digest.as_str());
        let query = serde_json::json!({"per_page": PER_PAGE});
        let list: types::AttestationList = self
            .fetch("Failed to fetch attestations", &route, Some(query))
            .await?;
        Ok(list
            .attestations
            .into_iter()
            .filter_map(|attestation| attestation.bundle?.dsse_envelope)
            .map(|envelope| envelope.payload)
            .collect())
    }

    pub(crate) async fn fetch_run(
        &self, owner: &str, repo: &str, run_id: u64,
    ) -> PluginResult<types::Run> {
        let route = format!("/repos/{owner}/{repo}/actions/runs/{run_id}");
        self.fetch("Failed to fetch run", &route, None).await
    }

    pub(super) async fn workflow_scopes(
        &self, owner: &str, repo: &str, workflows: &[WorkFlow],
    ) -> PluginResult<HashMap<WorkflowId, Vec<PathScope>>> {
        let requests: Vec<_> = workflows
            .iter()
            .map(|workflow| async move {
                self.workflow_scope(owner, repo, workflow)
                    .await
                    .map(|scopes| (workflow.id, scopes))
            })
            .collect();
        join_buffered(requests, FETCH_CONCURRENCY)
            .await
            .into_iter()
            .collect()
    }

    async fn workflow_scope(
        &self, owner: &str, repo: &str, workflow: &WorkFlow,
    ) -> PluginResult<Vec<PathScope>> {
        let cached = locked(&self.scope_cache)
            .get(&workflow.id.0)
            .filter(|(updated_at, _)| *updated_at == workflow.updated_at)
            .map(|(_, scopes)| scopes.clone());
        if let Some(scopes) = cached {
            return Ok(scopes);
        }

        let scopes = if workflow.path.starts_with(config::DYNAMIC_WORKFLOW_PREFIX) {
            Vec::new()
        } else {
            match self
                .retry_policy
                .retry(|| self.read_workflow_file(owner, repo, &workflow.path))
                .await
            {
                Ok(source) => paths::trigger_scopes(&source),
                Err(error) if error.is_transient() => return Err(error),
                Err(error) => {
                    tracing::debug!(
                        workflow = %workflow.path,
                        error = %error,
                        "Workflow file unavailable, reporting no path filter"
                    );
                    Vec::new()
                }
            }
        };
        locked(&self.scope_cache).insert(workflow.id.0, (workflow.updated_at, scopes.clone()));
        Ok(scopes)
    }

    pub(crate) async fn run_targets(
        &self, owner: &str, repo: &str, host: Option<&str>, runs: &[types::Run],
    ) -> PluginResult<Vec<Vec<TargetObservation>>> {
        let Some(cluster) =
            host.and_then(|host| ClusterId::from_host(&format!("{host}/{owner}/{repo}")))
        else {
            return Ok(vec![Vec::new(); runs.len()]);
        };
        let requests: Vec<_> = runs
            .iter()
            .map(|run| self.run_targets_of(owner, repo, &cluster, run))
            .collect();
        join_buffered(requests, FETCH_CONCURRENCY)
            .await
            .into_iter()
            .collect()
    }

    async fn run_targets_of(
        &self, owner: &str, repo: &str, cluster: &ClusterId, run: &types::Run,
    ) -> PluginResult<Vec<TargetObservation>> {
        let finished = run.status == "completed";
        if finished {
            if let Some(cached) = locked(&self.target_cache).get(&run.id.0).cloned() {
                return Ok(cached);
            }
        }

        let route = format!("/repos/{owner}/{repo}/deployments");
        let query = serde_json::json!({"sha": run.head_sha, "per_page": PER_PAGE});
        let deployments: Vec<types::Deployment> = match self
            .fetch("Failed to fetch deployments", &route, Some(query))
            .await
        {
            Ok(deployments) => deployments,
            Err(error) if error.is_transient() => return Err(error),
            Err(error) => {
                tracing::debug!(
                    run_id = run.id.0,
                    error = %error,
                    "Deployments unavailable, reporting no targets"
                );
                Vec::new()
            }
        };

        let targets =
            correlation::deployment_targets(cluster, run.created_at, run.updated_at, &deployments);
        if finished {
            locked(&self.target_cache).insert(run.id.0, targets.clone());
        }
        Ok(targets)
    }
}
