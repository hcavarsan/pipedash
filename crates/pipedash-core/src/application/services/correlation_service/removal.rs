use std::collections::BTreeSet;
use std::sync::PoisonError;

use super::resolve::finish;
use super::world::Affected;
use super::CorrelationService;
use crate::domain::pipeline_id::SEPARATOR;
use crate::domain::DomainResult;

impl CorrelationService {
    pub(super) fn forget_pipelines(&self, owns: impl Fn(&str) -> bool) {
        self.enrich.forget_provider(&owns);
        self.refetching
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|pipeline, _| !owns(pipeline));
    }

    pub(super) async fn provider_removed_impl(&self, provider_id: i64) -> DomainResult<()> {
        let id = provider_id.to_string();
        let owns = |pipeline: &str| pipeline.split(SEPARATOR).nth(1) == Some(id.as_str());
        self.forget_pipelines(owns);

        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = async {
            tx.delete_observed_transitions_by_provider(provider_id)
                .await?;
            tx.delete_observation_windows_by_provider(provider_id)
                .await?;
            let mut pipelines: BTreeSet<String> = BTreeSet::new();
            pipelines.extend(
                tx.list_run_evidence()
                    .await?
                    .into_iter()
                    .map(|row| row.pipeline_id),
            );
            pipelines.extend(
                tx.list_log_scans()
                    .await?
                    .into_iter()
                    .map(|row| row.pipeline_id),
            );
            for pipeline in pipelines.iter().filter(|p| owns(p)) {
                tx.delete_run_evidence_by_pipeline(pipeline).await?;
                tx.delete_log_scans_by_pipeline(pipeline).await?;
            }
            let world = self.load_world(&mut tx).await?;
            self.resolve_in(&mut tx, &world, &Affected::everything(), None)
                .await
        }
        .await;
        let resolved = finish(tx, result).await?;
        self.settle(resolved);
        self.changed();
        self.flush_events().await;
        Ok(())
    }
}
