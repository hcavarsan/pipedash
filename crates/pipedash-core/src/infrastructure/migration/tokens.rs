use super::{
    MigrationOptions,
    MigrationOrchestrator,
    MigrationResult,
    VAULT_PASSWORD_ENV_VAR,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::config::StorageManager;

fn get_vault_password(options: &MigrationOptions) -> DomainResult<String> {
    if let Ok(password) = std::env::var(VAULT_PASSWORD_ENV_VAR) {
        if !password.is_empty() {
            return Ok(password);
        }
    }

    options.token_password.clone().ok_or_else(|| {
        DomainError::InvalidConfig(format!(
            "Vault password required for token migration. Set {} environment variable or provide token_password in options.",
            VAULT_PASSWORD_ENV_VAR
        ))
    })
}

impl MigrationOrchestrator {
    pub(super) async fn migrate_tokens(
        &self, options: &MigrationOptions, target_manager: &StorageManager,
        result: &mut MigrationResult,
    ) -> DomainResult<()> {
        tracing::info!("[Migration] Starting token migration...");
        let password = get_vault_password(options)?;
        tracing::info!(
            "[Migration] Vault password retrieved: {} chars",
            password.len()
        );

        tracing::info!("[Migration] Exporting tokens from source token store...");
        let encrypted_blob = self.source_token_store.export_encrypted(&password).await?;
        tracing::info!(
            "[Migration] Exported encrypted blob: {} bytes",
            encrypted_blob.len()
        );

        if result.provider_id_mapping.is_empty() {
            tracing::info!("[Migration] Using direct import path (no ID remapping)");
            tracing::info!(
                "[Migration] Importing encrypted blob ({} bytes) to target token store...",
                encrypted_blob.len()
            );
            target_manager
                .token_store()
                .await
                .import_encrypted(&encrypted_blob, &password)
                .await?;
            tracing::info!("[Migration] Direct import completed");
        } else {
            self.remap_tokens(target_manager, result).await?;
        }

        let count: usize = target_manager
            .token_store()
            .await
            .get_all_tokens()
            .await?
            .len();
        tracing::info!(
            "[Migration] Verification: {} tokens now in target database",
            count
        );
        result.stats.tokens_migrated = count;
        Ok(())
    }

    async fn remap_tokens(
        &self, target_manager: &StorageManager, result: &MigrationResult,
    ) -> DomainResult<()> {
        tracing::info!(
            "[Migration] Using provider ID remapping path ({} mappings)",
            result.provider_id_mapping.len()
        );

        let source_tokens = self.source_token_store.get_all_tokens().await?;
        tracing::info!(
            "[Migration] Retrieved {} tokens from source",
            source_tokens.len()
        );

        let target_tokens = target_manager.token_store().await;
        let mut migrated_count = 0;
        for (old_id, token) in source_tokens {
            let Some(&new_id) = result.provider_id_mapping.get(&old_id) else {
                let source_providers = self.source_config_backend.list_providers().await?;
                return Err(DomainError::DataConsistency(format!(
                    "Cannot migrate token for provider ID {} - provider not found in remapping.\n\n\
                    This indicates data corruption or inconsistency.\n\n\
                    Source provider IDs: {:?}\n\
                    Remapping table: {:?}\n\n\
                    This token would be lost if migration continued.\n\
                    Please investigate why this provider's configuration wasn't migrated.",
                    old_id,
                    source_providers.iter().filter_map(|p| p.id).collect::<Vec<_>>(),
                    result.provider_id_mapping
                )));
            };
            tracing::debug!(
                "[Migration] Storing token for provider {} → {} (token length: {})",
                old_id,
                new_id,
                token.len()
            );
            target_tokens.store_token(new_id, &token).await?;
            migrated_count += 1;
            tracing::info!(
                "[Migration] Successfully stored token for provider {} (remapped to {})",
                old_id,
                new_id
            );
        }
        tracing::info!(
            "[Migration] Migrated {} tokens via ID remapping",
            migrated_count
        );
        Ok(())
    }
}
