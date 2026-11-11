use std::collections::HashSet;

use pipedash_plugin_api::{
    ArtifactRepository,
    Digest,
    ImageRef,
};

use super::registry::Attested;
use super::Pending;
use crate::application::services::correlation_service::CorrelationService;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::DomainResult;

impl CorrelationService {
    pub(super) async fn provider_attestations(
        &self, repository: &ArtifactRepository, digest: &Digest,
    ) -> (Vec<(ProviderLease, Attested)>, bool) {
        let mut found = Vec::new();
        let mut asked = false;
        let mut complete = true;
        for config in self.provider_configs().await {
            let Some(id) = config.id else {
                continue;
            };
            let Ok(lease) = self.provider_service.acquire(id).await else {
                continue;
            };
            if lease.provider.source_hosts(&config.config).is_empty() {
                continue;
            }
            if self.provider_service.check_quota(&lease).await.is_some() {
                complete = false;
                continue;
            }
            asked = true;
            let provenance = lease.provider.artifact_provenance(repository, digest).await;
            match provenance {
                Ok(Some(provenance)) => found.push((
                    lease,
                    Attested {
                        repo: Some(provenance.source.repo),
                        commit: provenance.source.commit,
                        invocation: None,
                        run: provenance.pipeline_id.zip(provenance.run_number),
                    },
                )),
                Ok(None) => {}
                Err(error) => {
                    complete = false;
                    tracing::debug!(%error, provider_id = id, "Provenance unavailable");
                }
            }
        }
        (found, asked && complete)
    }

    pub(super) async fn ask_providers(&self, pending: &Pending) -> DomainResult<HashSet<String>> {
        let artifact = &pending.artifact;
        let (Some(image), Some(digest)) = (
            ImageRef::parse(&artifact.repository),
            Digest::parse(&artifact.digest),
        ) else {
            self.enrich.mark_asked(&artifact.digest);
            return Ok(HashSet::new());
        };
        let (found, settled) = self.provider_attestations(&image.repository, &digest).await;
        self.record_leased(pending, found, settled).await
    }

    pub(super) async fn record_leased(
        &self, pending: &Pending, found: Vec<(ProviderLease, Attested)>, mut settled: bool,
    ) -> DomainResult<HashSet<String>> {
        let artifact = &pending.artifact;
        let mut touched = HashSet::new();
        for (lease, attested) in found {
            let run = attested.run.clone();
            let recorded = self
                .provider_service
                .commit(&lease, || {
                    self.record_attested(&artifact.digest, &artifact.repository, attested, pending)
                })
                .await;
            let Some(recorded) = recorded else {
                settled = false;
                continue;
            };
            touched.extend(recorded?);
            if let Some((pipeline, run_number)) = &run {
                settled &= self.run_stored_or_requested(pipeline, *run_number).await?;
            }
        }
        if settled {
            self.enrich.mark_asked(&artifact.digest);
        }
        Ok(touched)
    }

    async fn run_stored_or_requested(&self, pipeline: &str, run_number: i64) -> DomainResult<bool> {
        let stored = self
            .store
            .conn()
            .get_run_evidence(pipeline, run_number)
            .await?
            .is_some();
        if stored {
            return Ok(true);
        }
        match self.enrich.backfill_state(pipeline) {
            Some(finished) => Ok(finished),
            None => {
                self.backfill_runs([pipeline.to_string()]);
                Ok(false)
            }
        }
    }
}
