use pipedash_plugin_api::*;

use super::GitHubPlugin;
use crate::client::GitHubClient;
use crate::config;
use crate::correlation::provenance::Attested;
use crate::correlation::{
    self,
    provenance,
};

const GHCR_HOST: &str = "ghcr.io";

impl GitHubPlugin {
    fn repo_ref(repo: &RepoId) -> PluginResult<(String, String)> {
        config::parse_repo(repo.path()).ok_or_else(|| {
            PluginError::InvalidConfig(format!("Invalid repository: {}", repo.as_str()))
        })
    }

    pub(super) async fn resolve_commit_impl(
        &self, repo: &RepoId, rev: &str,
    ) -> PluginResult<CommitInfo> {
        let client = self.client()?;
        let (owner, name) = Self::repo_ref(repo)?;
        correlation::commit_info(client.fetch_commit(&owner, &name, rev).await?)
    }

    pub(super) async fn compare_commits_impl(
        &self, repo: &RepoId, base: &CommitSha, head: &CommitSha,
    ) -> PluginResult<CommitComparison> {
        let client = self.client()?;
        let (owner, name) = Self::repo_ref(repo)?;
        let comparison = client
            .compare_commits(&owner, &name, base.as_str(), head.as_str())
            .await?;
        Ok(correlation::commit_comparison(comparison))
    }

    pub(super) async fn commit_changes_impl(
        &self, repo: &RepoId, sha: &CommitSha,
    ) -> PluginResult<Vec<ChangedFile>> {
        let client = self.client()?;
        let (owner, name) = Self::repo_ref(repo)?;
        let commit = client.fetch_commit(&owner, &name, sha.as_str()).await?;
        Ok(correlation::changed_files(commit))
    }

    pub(super) async fn artifact_provenance_impl(
        &self, digest: &Digest,
    ) -> PluginResult<Option<Provenance>> {
        let client = self.client()?;
        let mut repositories = config::get_repositories(&self.config);
        repositories.sort();

        for full_name in repositories {
            let Some((owner, repo)) = config::parse_repo(&full_name) else {
                continue;
            };
            let payloads = match client.attestation_payloads(&owner, &repo, digest).await {
                Ok(payloads) => payloads,
                Err(error) if error.is_transient() => return Err(error),
                Err(error) => {
                    tracing::debug!(
                        repository = %full_name,
                        error = %error,
                        "Attestations unavailable for repository"
                    );
                    continue;
                }
            };
            if let Some(attested) = payloads
                .iter()
                .find_map(|payload| provenance::decode(payload))
            {
                return Self::provenance_of(client, attested).await.map(Some);
            }
        }
        Ok(None)
    }

    async fn provenance_of(client: &GitHubClient, attested: Attested) -> PluginResult<Provenance> {
        let run = match &attested.invocation {
            Some(invocation) => match client
                .fetch_run(&invocation.owner, &invocation.repo, invocation.run_id)
                .await
            {
                Ok(run) => Some((
                    config::pipeline_id(&invocation.owner, &invocation.repo, run.workflow_id.0),
                    run.run_number,
                )),
                Err(error) if error.is_transient() => return Err(error),
                Err(error) => {
                    tracing::debug!(error = %error, "Attested run unavailable");
                    None
                }
            },
            None => None,
        };
        let (pipeline_id, run_number) = match run {
            Some((pipeline_id, run_number)) => (Some(pipeline_id), Some(run_number)),
            None => (None, None),
        };
        Ok(Provenance {
            pipeline_id,
            run_number,
            source: attested.source,
        })
    }

    pub(super) async fn registry_auth_impl(
        &self, host: &str,
    ) -> PluginResult<Option<RegistryAuth>> {
        let client = self.client()?;
        let hosted = config::source_host(&self.config).as_deref() == Some("github.com");
        Ok(
            (hosted && host.eq_ignore_ascii_case(GHCR_HOST)).then(|| RegistryAuth {
                username: None,
                secret: client.token().to_string(),
                bearer: true,
                token_hosts: Vec::new(),
            }),
        )
    }
}
