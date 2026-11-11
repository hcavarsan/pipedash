use std::collections::{
    BTreeSet,
    HashSet,
};

use chrono::Utc;
use pipedash_plugin_api::correlation::oci::{
    dsse_provenance,
    OciClient,
};
use pipedash_plugin_api::{
    ArtifactRepository,
    ClientOptions,
    CommitSha,
    Digest,
    ImageRef,
    PluginError,
    RegistryAuth,
    RepoId,
};

use super::super::vocab::{
    commit_node_id,
    ARTIFACT,
    ATTESTATION,
    ATTESTATION_COMMIT,
    BUILT_FROM,
    COMMIT,
    ORIGIN_ATTESTATION,
    REL_PRODUCED,
};
use super::super::CorrelationService;
use super::Pending;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::application::Confidence;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::{
    ArtifactObservationRow,
    CorrelationEdgeRow,
    PIPELINE_LEVEL,
};
use crate::infrastructure::providers::PluginAdapter;

const LABEL_SOURCE: &str = "org.opencontainers.image.source";
const LABEL_REVISION: &str = "org.opencontainers.image.revision";
const ATTESTATION_TYPES: [&str; 2] = [
    "application/vnd.in-toto+json",
    "application/vnd.dev.sigstore.bundle.v0.3+json",
];

pub(super) struct Attested {
    pub(super) repo: Option<RepoId>,
    pub(super) commit: Option<CommitSha>,
    pub(super) invocation: Option<String>,
    pub(super) run: Option<(String, i64)>,
}

pub(super) fn names_run(invocation: &str, logs_url: &str) -> bool {
    !logs_url.is_empty()
        && invocation
            .strip_prefix(logs_url)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

impl CorrelationService {
    pub(super) async fn read_registries(&self) -> DomainResult<HashSet<String>> {
        let mut touched = HashSet::new();
        for pending in self
            .unproduced(|artifact| !self.enrich.asked(&artifact.digest))
            .await?
        {
            touched.extend(self.ask_providers(&pending).await?);
        }
        if !self.config_state.get().await.correlation.registry_read {
            return Ok(touched);
        }
        for pending in self
            .unproduced(|artifact| artifact.registry_checked_at.is_none())
            .await?
        {
            if !self.enrich.due(&pending.artifact.digest) {
                continue;
            }
            match self.read_registry(&pending).await {
                Ok(subjects) => touched.extend(subjects),
                Err(error) => {
                    tracing::debug!(%error, digest = %pending.artifact.digest, "Registry read failed");
                    self.enrich.defer(pending.artifact.digest.clone());
                }
            }
        }
        Ok(touched)
    }

    pub(super) async fn registry_access(
        &self, host: &str,
    ) -> Option<(RegistryAuth, ProviderLease)> {
        for config in self.provider_configs().await {
            let Some(id) = config.id else {
                continue;
            };
            let Ok(lease) = self.provider_service.acquire(id).await else {
                continue;
            };
            let granted = lease.provider.registry_auth(host).await;
            match granted {
                Ok(Some(auth)) => return Some((auth, lease)),
                Ok(None) => {}
                Err(error) => {
                    tracing::debug!(%error, provider_id = id, "Registry auth unavailable")
                }
            }
        }
        None
    }

    pub(super) fn oci_client(&self) -> DomainResult<OciClient> {
        let http = self.http.client_for(&ClientOptions::default())?;
        Ok(OciClient::new(reqwest::Client::clone(&http)))
    }

    async fn read_registry(&self, pending: &Pending) -> DomainResult<HashSet<String>> {
        let artifact = &pending.artifact;
        let (Some(image), Some(digest)) = (
            ImageRef::parse(&artifact.repository),
            Digest::parse(&artifact.digest),
        ) else {
            return Err(DomainError::InvalidConfig(format!(
                "Unreadable artifact {}",
                artifact.digest
            )));
        };
        let repository = image.repository;
        let access = self.registry_access(repository.host()).await;
        let auth = access.as_ref().map(|(auth, _)| auth);
        let client = self.oci_client()?;
        let mut attested = Vec::new();
        let mut label_source = None;
        let mut label_revision = None;
        match read_labels(&client, &repository, &digest, auth).await {
            Ok((source, revision)) => {
                label_source = source;
                label_revision = revision;
            }
            Err(error) if error.is_transient() => return Err(PluginAdapter::map_error(error)),
            Err(error) => tracing::debug!(%error, "Registry labels unreadable"),
        }
        match read_attestations(&client, &repository, &digest, auth).await {
            Ok(found) => attested.extend(found),
            Err(error) if error.is_transient() => return Err(PluginAdapter::map_error(error)),
            Err(error) => tracing::debug!(%error, "Registry referrers unreadable"),
        }
        match access {
            None => {
                self.settle_registry(pending, attested, label_source, label_revision)
                    .await
            }
            Some((_, lease)) => {
                let settled = self
                    .provider_service
                    .commit(&lease, || {
                        self.settle_registry(pending, attested, label_source, label_revision)
                    })
                    .await;
                settled.unwrap_or_else(|| Ok(HashSet::new()))
            }
        }
    }

    async fn settle_registry(
        &self, pending: &Pending, attested: Vec<Attested>, label_source: Option<RepoId>,
        label_revision: Option<CommitSha>,
    ) -> DomainResult<HashSet<String>> {
        let artifact = &pending.artifact;
        let mut touched: HashSet<String> = pending.consumers.iter().cloned().collect();
        for found in attested {
            touched.extend(
                self.record_attested(
                    artifact.digest.as_str(),
                    &artifact.repository,
                    found,
                    pending,
                )
                .await?,
            );
        }
        self.store
            .conn()
            .mark_artifact_registry_checked(
                &artifact.digest,
                label_source.as_ref().map(RepoId::as_str),
                label_revision.as_ref().map(CommitSha::as_str),
                Utc::now(),
            )
            .await?;
        Ok(touched)
    }

    pub(super) async fn record_attested(
        &self, digest: &str, repository: &str, found: Attested, pending: &Pending,
    ) -> DomainResult<HashSet<String>> {
        let mut touched = HashSet::new();
        if let (Some(repo), None) = (&found.repo, &found.run) {
            let repos = BTreeSet::from([repo.as_str().to_string()]);
            self.backfill_for_repos(&repos).await?;
        }
        if let (Some(repo), Some(commit)) = (&found.repo, &found.commit) {
            let now = Utc::now();
            self.store
                .conn()
                .upsert_edge(&CorrelationEdgeRow {
                    id: 0,
                    from_kind: ARTIFACT.to_string(),
                    from_id: digest.to_string(),
                    to_kind: COMMIT.to_string(),
                    to_id: commit_node_id(repo.as_str(), commit.as_str()),
                    relation: BUILT_FROM.to_string(),
                    confidence: Confidence::Exact.as_str().to_string(),
                    rule: ATTESTATION_COMMIT.to_string(),
                    evidence_json: serde_json::json!({ "source": ATTESTATION }).to_string(),
                    first_seen: now,
                    last_seen: now,
                })
                .await?;
        }
        if let Some((subject, run_number, provider_id)) = self.producing_run(&found).await? {
            self.attest_production(provider_id, &subject, run_number, digest, repository)
                .await?;
            touched.insert(subject);
        }
        touched.extend(pending.consumers.iter().cloned());
        Ok(touched)
    }

    async fn producing_run(&self, found: &Attested) -> DomainResult<Option<(String, i64, i64)>> {
        let mut conn = self.store.conn();
        if let Some((pipeline, run_number)) = &found.run {
            if conn
                .get_run_evidence(pipeline, *run_number)
                .await?
                .is_none()
            {
                return Ok(None);
            }
            let provider_id = conn
                .list_source_observations_by_subject(pipeline)
                .await?
                .first()
                .map(|row| row.provider_id);
            return Ok(provider_id.map(|id| (pipeline.clone(), *run_number, id)));
        }
        let (Some(repo), Some(commit), Some(invocation)) =
            (&found.repo, &found.commit, &found.invocation)
        else {
            return Ok(None);
        };
        let rows = conn
            .list_source_observations_by_repo_commit(repo.as_str(), commit.as_str())
            .await?;
        let mut named = Vec::new();
        for row in rows
            .into_iter()
            .filter(|row| row.run_number != PIPELINE_LEVEL)
        {
            let run = self
                .repository
                .get_cached_run(&row.subject_id, row.run_number)
                .await?;
            if run.is_some_and(|run| names_run(invocation, &run.logs_url)) {
                named.push((row.subject_id, row.run_number, row.provider_id));
            }
        }
        Ok((named.len() == 1).then(|| named.remove(0)))
    }

    async fn attest_production(
        &self, provider_id: i64, subject: &str, run_number: i64, digest: &str, repository: &str,
    ) -> DomainResult<()> {
        let mut conn = self.store.conn();
        let mut rows: Vec<ArtifactObservationRow> = conn
            .list_artifact_observations_by_subject(subject)
            .await?
            .into_iter()
            .filter(|row| row.run_number == run_number && row.origin == ORIGIN_ATTESTATION)
            .collect();
        if rows.iter().any(|row| row.digest.as_deref() == Some(digest)) {
            return Ok(());
        }
        rows.push(ArtifactObservationRow {
            provider_id,
            subject_id: subject.to_string(),
            run_number,
            relation: REL_PRODUCED.to_string(),
            ordinal: 0,
            repository: repository.to_string(),
            digest: Some(digest.to_string()),
            tag: None,
            target_ordinal: None,
            origin: ORIGIN_ATTESTATION.to_string(),
        });
        conn.replace_artifact_observations(subject, run_number, ORIGIN_ATTESTATION, &rows)
            .await
    }
}

async fn read_labels(
    client: &OciClient, repository: &ArtifactRepository, digest: &Digest,
    auth: Option<&RegistryAuth>,
) -> Result<(Option<RepoId>, Option<CommitSha>), PluginError> {
    let (_, manifest) = client.manifest(repository, digest.as_str(), auth).await?;
    let labels = client.config_labels(repository, &manifest, auth).await?;
    Ok((
        labels
            .get(LABEL_SOURCE)
            .and_then(|value| RepoId::parse(value)),
        labels
            .get(LABEL_REVISION)
            .and_then(|value| CommitSha::parse(value)),
    ))
}

async fn read_attestations(
    client: &OciClient, repository: &ArtifactRepository, digest: &Digest,
    auth: Option<&RegistryAuth>,
) -> Result<Vec<Attested>, PluginError> {
    let mut found = Vec::new();
    for referrer in client.referrers(repository, digest, auth).await? {
        let attestation = referrer
            .artifact_type
            .as_deref()
            .is_some_and(|kind| ATTESTATION_TYPES.contains(&kind));
        if !attestation {
            continue;
        }
        let (_, manifest) = client
            .manifest(repository, referrer.digest.as_str(), auth)
            .await?;
        for layer in manifest.layers {
            let bundle = client.blob(repository, &layer.digest, auth).await?;
            if let Some(provenance) = dsse_provenance(&bundle) {
                found.push(Attested {
                    repo: provenance.repo,
                    commit: provenance.commit,
                    invocation: provenance.invocation,
                    run: None,
                });
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
