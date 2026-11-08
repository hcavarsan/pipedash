use std::collections::BTreeMap;

use super::remap::remap_pipeline_id;
use super::{
    MigrationOrchestrator,
    MigrationResult,
};
use crate::domain::{
    DomainResult,
    MetricsQuery,
};
use crate::infrastructure::config::StorageManager;
use crate::infrastructure::database::{
    MetricsRepository,
    RecordMetrics,
};

impl MigrationOrchestrator {
    pub(super) async fn migrate_metrics(
        &self, target_manager: &StorageManager, result: &mut MigrationResult,
    ) -> DomainResult<()> {
        let target_metrics = MetricsRepository::new_from_pool(target_manager.database_pool());
        let pipeline_query = |pipeline_id: &str| MetricsQuery {
            pipeline_id: Some(pipeline_id.to_string()),
            limit: None,
            ..Default::default()
        };
        let mut pipelines = 0;
        for source_id in self.source_metrics.list_pipeline_ids().await? {
            let Some(target_id) = remap_pipeline_id(&source_id, &result.provider_id_mapping) else {
                continue;
            };
            let last_run = self
                .source_metrics
                .get_last_processed_run(&source_id)
                .await?;
            let mut runs: BTreeMap<i64, RecordMetrics> = BTreeMap::new();
            for entry in self
                .source_metrics
                .query_metrics(pipeline_query(&source_id))
                .await?
            {
                let slot = entry.metric_type as usize;
                let record = runs
                    .entry(entry.run_number)
                    .or_insert_with(|| RecordMetrics {
                        run_number: entry.run_number,
                        run_hash: entry.run_hash.clone().unwrap_or_default(),
                        entries: [None, None, None],
                    });
                record.entries[slot] = Some(entry);
            }
            let records: Vec<_> = runs.into_values().collect();
            target_metrics
                .refresh_record_metrics(&target_id, &records)
                .await?;
            result.stats.metrics_migrated += records
                .iter()
                .map(|record| record.entries.iter().flatten().count())
                .sum::<usize>();
            pipelines += 1;

            let Some(run) = last_run else {
                continue;
            };
            if target_metrics
                .get_last_processed_run(&target_id)
                .await?
                .is_none_or(|target_run| target_run < run)
            {
                target_metrics
                    .update_last_processed_run(&target_id, run)
                    .await?;
            }
        }
        tracing::info!(
            "[Migration] Migrated {} metric entries for {} pipelines",
            result.stats.metrics_migrated,
            pipelines
        );
        Ok(())
    }
}
