use std::collections::HashMap;

use super::remap::remap_pipeline_id;
use super::{
    MigrationOptions,
    MigrationOrchestrator,
};
use crate::domain::DomainResult;
use crate::infrastructure::config::StorageManager;
use crate::infrastructure::database::PipelineViewRepository;

impl MigrationOrchestrator {
    pub(super) async fn migrate_views(
        &self, options: &MigrationOptions, target_manager: &StorageManager,
        providers: &HashMap<i64, i64>,
    ) -> DomainResult<()> {
        let mut export = PipelineViewRepository::new(self.source_pool.clone())
            .export()
            .await?;
        let remap = |pipeline_id: &mut String, provider_id: &mut i64| match (
            remap_pipeline_id(pipeline_id, providers),
            providers.get(provider_id),
        ) {
            (Some(next), Some(target)) => {
                *pipeline_id = next;
                *provider_id = *target;
                true
            }
            _ => false,
        };
        export
            .members
            .retain_mut(|m| remap(&mut m.pipeline_id, &mut m.provider_id));
        for view in &mut export.views {
            view.rules.provider_ids = view
                .rules
                .provider_ids
                .iter()
                .filter_map(|id| providers.get(id).copied())
                .collect();
        }
        PipelineViewRepository::new(target_manager.database_pool())
            .import(&export, options.clean_target)
            .await?;
        tracing::info!("[Migration] Migrated {} pipeline views", export.views.len());
        Ok(())
    }
}
