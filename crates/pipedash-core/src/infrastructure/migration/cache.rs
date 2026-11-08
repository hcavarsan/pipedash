use super::remap::{
    remap_pipeline_id,
    same_database,
};
use super::{
    MigrationOrchestrator,
    MigrationPlan,
    MigrationResult,
};
use crate::domain::DomainResult;
use crate::infrastructure::config::StorageManager;
use crate::infrastructure::database::Repository;

impl MigrationOrchestrator {
    pub(super) async fn migrate_cache(
        &self, plan: &MigrationPlan, target_manager: &StorageManager, result: &mut MigrationResult,
    ) -> DomainResult<()> {
        let objects = self.source_cache_backend.list(None).await?;
        tracing::info!(
            "[Migration] Migrating {} cache entries from {:?} to {:?}",
            objects.len(),
            self.source_config.data_dir().join("cache"),
            plan.to.data_dir().join("cache")
        );
        for obj in objects {
            let data = self.source_cache_backend.get(&obj.key).await?;
            target_manager
                .cache_backend()
                .put(&obj.key, &data, obj.content_type.as_deref())
                .await?;
            result.stats.cache_entries_migrated += 1;
        }

        if same_database(&plan.from, &plan.to) {
            return Ok(());
        }

        self.migrate_run_history(target_manager, result).await?;
        self.migrate_metrics(target_manager, result).await
    }

    async fn migrate_run_history(
        &self, target_manager: &StorageManager, result: &mut MigrationResult,
    ) -> DomainResult<()> {
        let source_repo = Repository::new(
            self.source_config_backend.clone(),
            self.source_pool.clone(),
            self.source_token_store.clone(),
        );
        let target_repo = Repository::new(
            target_manager.config_backend(),
            target_manager.database_pool(),
            target_manager.token_store().await,
        );
        for source_id in source_repo.list_run_history_pipeline_ids().await? {
            let Some(target_id) = remap_pipeline_id(&source_id, &result.provider_id_mapping) else {
                continue;
            };
            let mut runs = source_repo
                .get_cached_run_history(
                    &source_id,
                    i64::MAX as usize,
                    pipedash_plugin_api::RunHistoryOrder::RunNumber,
                )
                .await?;
            for run in &mut runs {
                run.pipeline_id.clone_from(&target_id);
            }
            target_repo.cache_run_history(&target_id, &runs).await?;
            result.stats.cache_entries_migrated += runs.len();
        }
        Ok(())
    }
}
