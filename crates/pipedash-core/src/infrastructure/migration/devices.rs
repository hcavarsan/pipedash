use std::collections::HashMap;

use super::remap::remap_preferences;
use super::{
    MigrationOptions,
    MigrationOrchestrator,
};
use crate::domain::DomainResult;
use crate::infrastructure::config::StorageManager;
use crate::infrastructure::database::{
    DevicePushRepository,
    DeviceRepository,
    PushRepository,
};

impl MigrationOrchestrator {
    pub(super) async fn migrate_devices(
        &self, options: &MigrationOptions, target_manager: &StorageManager,
        providers: &HashMap<i64, i64>,
    ) -> DomainResult<()> {
        let subscriptions = PushRepository::new(self.source_pool.clone()).list().await?;
        let target_push = PushRepository::new(target_manager.database_pool());
        target_push.import(&subscriptions).await?;
        if options.clean_target {
            for stale in target_push.list().await? {
                if !subscriptions.iter().any(|s| s.id == stale.id) {
                    target_push.delete(&stale.id).await?;
                }
            }
        }
        tracing::info!(
            "[Migration] Migrated {} push subscriptions",
            subscriptions.len()
        );

        let source_devices = DeviceRepository::new(self.source_pool.clone());
        let devices = source_devices.list().await?;
        let target_devices = DeviceRepository::new(target_manager.database_pool());
        target_devices.import(&devices).await?;
        for (id, preferences) in source_devices.notification_preferences().await? {
            let preferences = remap_preferences(preferences, providers);
            target_devices
                .set_notification_preferences(&id, &preferences)
                .await?;
        }
        if options.clean_target {
            for stale in target_devices.list().await? {
                if !devices.iter().any(|d| d.id == stale.id) {
                    target_devices.delete(&stale.id).await?;
                }
            }
        }
        tracing::info!("[Migration] Migrated {} paired devices", devices.len());

        let pushes = DevicePushRepository::new(self.source_pool.clone())
            .list()
            .await?;
        let target_pushes = DevicePushRepository::new(target_manager.database_pool());
        target_pushes.import(&pushes).await?;
        if options.clean_target {
            for stale in target_pushes.list().await? {
                if !pushes.iter().any(|p| p.device_id == stale.device_id) {
                    target_pushes.delete(&stale.device_id).await?;
                }
            }
        }
        tracing::info!(
            "[Migration] Migrated {} device push registrations",
            pushes.len()
        );

        Ok(())
    }
}
