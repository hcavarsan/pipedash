use std::collections::HashMap;

use super::remap::remap_alert;
use super::{
    MigrationOptions,
    MigrationOrchestrator,
};
use crate::domain::{
    DeviceAlert,
    DomainResult,
};
use crate::infrastructure::config::StorageManager;
use crate::infrastructure::database::DeviceAlertRepository;

impl MigrationOrchestrator {
    pub(super) async fn migrate_alerts(
        &self, options: &MigrationOptions, target_manager: &StorageManager,
        providers: &HashMap<i64, i64>,
    ) -> DomainResult<()> {
        let alerts: Vec<DeviceAlert> = DeviceAlertRepository::new(self.source_pool.clone())
            .list()
            .await?
            .into_iter()
            .filter_map(|alert| remap_alert(alert, providers))
            .collect();
        let target_alerts = DeviceAlertRepository::new(target_manager.database_pool());
        if options.clean_target {
            target_alerts.delete_all().await?;
            target_alerts.import(&alerts).await?;
        } else {
            target_alerts.merge(&alerts).await?;
        }
        tracing::info!("[Migration] Migrated {} device alerts", alerts.len());
        Ok(())
    }
}
