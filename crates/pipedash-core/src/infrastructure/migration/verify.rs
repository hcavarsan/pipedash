use std::collections::HashMap;

use super::{
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
};
use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::config::StorageManager;

impl MigrationOrchestrator {
    pub(super) async fn verify_migration(
        &self, plan: &MigrationPlan, options: &MigrationOptions, target_manager: &StorageManager,
    ) -> DomainResult<()> {
        let source_providers = self.source_config_backend.list_providers().await?;

        if plan.migrate_configs && !source_providers.is_empty() {
            let target_providers: Vec<ProviderConfig> =
                target_manager.config_backend().list_providers().await?;
            Self::verify_providers(options, &source_providers, &target_providers)?;
        } else if plan.migrate_configs {
            tracing::info!("Skipping provider verification: source database is empty");
        }

        if plan.migrate_tokens {
            self.verify_tokens(options, target_manager).await?;
        }

        Ok(())
    }

    fn verify_providers(
        options: &MigrationOptions, source_providers: &[ProviderConfig],
        target_providers: &[ProviderConfig],
    ) -> DomainResult<()> {
        for source_provider in source_providers {
            let found = target_providers
                .iter()
                .any(|t| t.name == source_provider.name);
            if !found {
                return Err(DomainError::DatabaseError(format!(
                    "Provider '{}' from source was not found in target after migration",
                    source_provider.name
                )));
            }
        }

        if options.clean_target {
            if source_providers.len() != target_providers.len() {
                return Err(DomainError::DatabaseError(format!(
                    "Provider count mismatch after cleanup: source={}, target={}",
                    source_providers.len(),
                    target_providers.len()
                )));
            }
            tracing::info!(
                "[Migration] Verified exact match: {} providers",
                source_providers.len()
            );
        } else if target_providers.len() > source_providers.len() {
            tracing::info!(
                "[Migration] Merge complete: {} migrated, {} pre-existing kept (total: {})",
                source_providers.len(),
                target_providers.len() - source_providers.len(),
                target_providers.len()
            );
        } else {
            tracing::info!(
                "[Migration] Verified {} providers migrated successfully",
                source_providers.len()
            );
        }
        Ok(())
    }

    async fn verify_tokens(
        &self, options: &MigrationOptions, target_manager: &StorageManager,
    ) -> DomainResult<()> {
        let source_tokens = self.source_token_store.get_all_tokens().await?;
        if source_tokens.is_empty() {
            tracing::info!("Skipping token verification: source has no tokens");
            return Ok(());
        }

        let target_tokens: HashMap<i64, String> =
            target_manager.token_store().await.get_all_tokens().await?;

        if options.clean_target {
            if source_tokens.len() != target_tokens.len() {
                return Err(DomainError::DatabaseError(format!(
                    "Token count mismatch after cleanup: source={}, target={}",
                    source_tokens.len(),
                    target_tokens.len()
                )));
            }
            tracing::info!(
                "[Migration] Verified exact match: {} tokens",
                source_tokens.len()
            );
            return Ok(());
        }

        if target_tokens.len() < source_tokens.len() {
            return Err(DomainError::DatabaseError(format!(
                "Token count too low after migration: source={}, target={}",
                source_tokens.len(),
                target_tokens.len()
            )));
        }
        tracing::info!(
            "[Migration] Token migration complete: {} migrated (target has {} total)",
            source_tokens.len(),
            target_tokens.len()
        );
        Ok(())
    }
}
