use std::collections::HashSet;

use super::remap::{
    remap_pipeline_id,
    same_database,
};
use super::{
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
    MigrationResult,
};
use crate::domain::{
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::config::StorageManager;
use crate::infrastructure::database::MetricsRepository;

impl MigrationOrchestrator {
    pub(super) async fn migrate_configs(
        &self, plan: &MigrationPlan, options: &MigrationOptions, target_manager: &StorageManager,
        result: &mut MigrationResult,
    ) -> DomainResult<()> {
        tracing::info!(
            "[Migration] Exporting providers from source database: {:?}",
            self.source_config.db_path()
        );
        let export = self.source_config_backend.export_all().await?;
        result.stats.providers_migrated = export.providers.len();
        result.stats.permissions_migrated = export.permissions.len();

        tracing::info!(
            "[Migration] Importing {} providers to target database: {:?}",
            export.providers.len(),
            plan.to.db_path()
        );
        target_manager.config_backend().import_all(&export).await?;

        let source_providers = self.source_config_backend.list_providers().await?;
        let target_providers: Vec<ProviderConfig> =
            target_manager.config_backend().list_providers().await?;

        for source_provider in &source_providers {
            let Some(source_id) = source_provider.id else {
                continue;
            };
            let target_id = target_providers
                .iter()
                .find(|p| p.name == source_provider.name)
                .and_then(|p| p.id);
            if let Some(target_id) = target_id {
                result.provider_id_mapping.insert(source_id, target_id);
            }
        }

        let cross_database = !same_database(&plan.from, &plan.to);
        if cross_database {
            self.migrate_metrics_config(target_manager, result).await?;
            self.migrate_devices(options, target_manager, &result.provider_id_mapping)
                .await?;
            self.migrate_views(options, target_manager, &result.provider_id_mapping)
                .await?;
        }

        if options.clean_target {
            Self::clean_orphaned_providers(
                target_manager,
                &source_providers,
                &target_providers,
                result,
            )
            .await?;
        }

        if cross_database {
            self.migrate_alerts(options, target_manager, &result.provider_id_mapping)
                .await?;
        }

        Ok(())
    }

    async fn migrate_metrics_config(
        &self, target_manager: &StorageManager, result: &MigrationResult,
    ) -> DomainResult<()> {
        let mut metrics_config = self.source_metrics.export_config().await?;
        metrics_config.pipeline_configs.retain_mut(|config| {
            match remap_pipeline_id(&config.pipeline_id, &result.provider_id_mapping) {
                Some(pipeline_id) => {
                    config.pipeline_id = pipeline_id;
                    true
                }
                None => false,
            }
        });
        MetricsRepository::new_from_pool(target_manager.database_pool())
            .import_config(&metrics_config)
            .await
    }

    async fn clean_orphaned_providers(
        target_manager: &StorageManager, source_providers: &[ProviderConfig],
        target_providers: &[ProviderConfig], result: &mut MigrationResult,
    ) -> DomainResult<()> {
        let source_names: HashSet<&str> =
            source_providers.iter().map(|p| p.name.as_str()).collect();
        let orphaned: Vec<_> = target_providers
            .iter()
            .filter(|p| !source_names.contains(p.name.as_str()))
            .collect();

        for orphan in &orphaned {
            let Some(id) = orphan.id else {
                continue;
            };
            tracing::info!(
                "[Migration] Removing orphaned provider: {} (id={})",
                orphan.name,
                id
            );
            target_manager.config_backend().delete_provider(id).await?;
            let _ = target_manager.token_store().await.delete_token(id).await;
            result.stats.tokens_cleaned += 1;
        }

        if !orphaned.is_empty() {
            result.stats.providers_cleaned = orphaned.len();
            tracing::info!(
                "[Migration] Cleaned {} orphaned providers from target",
                orphaned.len()
            );
        }
        Ok(())
    }
}
