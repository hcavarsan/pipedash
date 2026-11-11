use pipedash_plugin_api::PermissionStatus;

use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::config_backend::StoredPermissions;
use crate::infrastructure::database::retry::retry_on_busy;
use crate::infrastructure::database::{
    DatabasePool,
    PipelineViewRepository,
};
use crate::infrastructure::ConfigState;

fn with_cleanup(error: DomainError, cleanup: DomainResult<()>) -> DomainError {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => {
            DomainError::InternalError(format!("{error}; cleanup failed: {cleanup_error}"))
        }
    }
}

impl Repository {
    pub async fn add_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        let provider_id = self.config_backend.create_provider(config).await?;

        if let Err(store_error) = self
            .token_store
            .store_token(provider_id, &config.token)
            .await
        {
            tracing::error!(
                provider_id = provider_id,
                error = %store_error,
                "Failed to store provider token - removing the provider"
            );
            let cleanup = self.config_backend.delete_provider(provider_id).await;

            return Err(with_cleanup(store_error, cleanup));
        }

        tracing::debug!(
            provider_id = provider_id,
            "Provider created and token stored successfully"
        );

        Ok(provider_id)
    }

    pub async fn get_provider(&self, id: i64) -> DomainResult<ProviderConfig> {
        let mut provider = self
            .config_backend
            .get_provider(id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound(id.to_string()))?;

        provider.token = self.stored_token(id).await?.unwrap_or_default();

        Ok(provider)
    }

    pub async fn store_token(&self, id: i64, token: &str) -> DomainResult<()> {
        self.token_store.store_token(id, token).await
    }

    pub async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        let mut providers = self.config_backend.list_providers().await?;

        for provider in &mut providers {
            if let Some(id) = provider.id {
                provider.token = self.token_store.get_token(id).await.unwrap_or_default();
            }
        }

        Ok(providers)
    }

    pub async fn list_provider_ids(&self) -> DomainResult<Vec<i64>> {
        Ok(self
            .config_backend
            .list_providers()
            .await?
            .into_iter()
            .filter_map(|provider| provider.id)
            .collect())
    }

    pub async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        let previous = self.get_provider(id).await?;
        self.config_backend.update_provider(id, config).await?;

        self.store_changed_token(id, config, &previous).await
    }

    pub async fn update_provider_with_version(
        &self, id: i64, config: &ProviderConfig, expected_version: i64,
    ) -> DomainResult<bool> {
        let previous = self.get_provider(id).await?;
        let pool = &self.cache_pool;
        let sql = format!(
            "UPDATE providers SET name = {}, provider_type = {}, config_json = {}, refresh_interval = {}, version = version + 1, updated_at = {} WHERE id = {} AND version = {}",
            pool.placeholder(1),
            pool.placeholder(2),
            pool.placeholder(3),
            pool.placeholder(4),
            pool.datetime_now(),
            pool.placeholder(5),
            pool.placeholder(6)
        );

        let updated = retry_on_busy(|| async {
            let config_json = serde_json::to_string(&config.config).map_err(|e| {
                DomainError::DatabaseError(format!("Failed to serialize config: {}", e))
            })?;

            let rows_affected = on_pool!(pool, p => sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                .bind(&config.name)
                .bind(&config.provider_type)
                .bind(&config_json)
                .bind(config.refresh_interval)
                .bind(id)
                .bind(expected_version)
                .execute(p)
                .await
                .map_err(|e| DomainError::DatabaseError(format!("Failed to update provider: {}", e)))?
                .rows_affected());

            Ok(rows_affected > 0)
        })
        .await?;

        if updated {
            self.store_changed_token(id, config, &previous).await?;
        }

        Ok(updated)
    }

    async fn stored_token(&self, id: i64) -> DomainResult<Option<String>> {
        match self.token_store.get_token(id).await {
            Ok(token) => Ok(Some(token)),
            Err(DomainError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn store_changed_token(
        &self, id: i64, config: &ProviderConfig, previous: &ProviderConfig,
    ) -> DomainResult<()> {
        if config.token == previous.token {
            return Ok(());
        }

        match self.token_store.store_token(id, &config.token).await {
            Ok(()) => Ok(()),
            Err(store_error) => {
                let restore = self.config_backend.update_provider(id, previous).await;

                Err(with_cleanup(store_error, restore))
            }
        }
    }

    async fn restore_token(
        &self, id: i64, token: Option<String>, error: DomainError,
    ) -> DomainError {
        let Some(token) = token else {
            return error;
        };
        let restore = self.token_store.store_token(id, &token).await;

        with_cleanup(error, restore)
    }

    pub async fn discard_provider(&self, id: i64, error: DomainError) -> DomainError {
        let cleanup = match self.token_store.delete_token(id).await {
            Ok(()) => self.config_backend.delete_provider(id).await,
            Err(e) => Err(e),
        };

        with_cleanup(error, cleanup)
    }

    pub async fn remove_provider(&self, id: i64, config_state: &ConfigState) -> DomainResult<()> {
        let config = self.config_backend.get_provider(id).await?;
        let token = self.stored_token(id).await?;
        let pipelines = self
            .get_cached_pipelines(Some(id))
            .await
            .unwrap_or_default();
        self.token_store.delete_token(id).await?;

        let removed = match &config {
            Some(config) => match config_state.remove_provider(&config.name).await {
                Ok(removed) => Some(removed),
                Err(error) => return Err(self.restore_token(id, token, error).await),
            },
            None => None,
        };

        if let Err(mut error) = self.config_backend.delete_provider(id).await {
            if let Some(removed) = removed {
                error = with_cleanup(error, config_state.restore_provider(removed).await);
            }

            return Err(self.restore_token(id, token, error).await);
        }

        let ids: Vec<String> = pipelines.into_iter().map(|pipeline| pipeline.id).collect();
        self.clear_removed_caches(id, &ids).await;
        if let Err(e) = PipelineViewRepository::new(self.cache_pool.clone())
            .remove_provider(id)
            .await
        {
            tracing::warn!("Failed to remove provider {id} from pipeline views: {e}");
        }

        Ok(())
    }

    async fn clear_removed_caches(&self, provider_id: i64, pipeline_ids: &[String]) {
        let history_sql = format!(
            "DELETE FROM run_history_cache WHERE pipeline_id = {}",
            self.cache_pool.placeholder(1)
        );
        let workflow_params_sql = format!(
            "DELETE FROM workflow_parameters_cache WHERE workflow_id LIKE {}",
            self.cache_pool.placeholder(1)
        );
        for pipeline_id in pipeline_ids {
            let _ = on_pool!(&self.cache_pool, p => sqlx::query(sqlx::AssertSqlSafe(history_sql.clone()))
                .bind(pipeline_id)
                .execute(p)
                .await
                .map(|_| ()));
            let _ = on_pool!(&self.cache_pool, p => sqlx::query(sqlx::AssertSqlSafe(workflow_params_sql.clone()))
                .bind(format!("{pipeline_id}%"))
                .execute(p)
                .await
                .map(|_| ()));
        }

        let pipelines_sql = format!(
            "DELETE FROM pipelines_cache WHERE provider_id = {}",
            self.cache_pool.placeholder(1)
        );
        let _ = on_pool!(&self.cache_pool, p => sqlx::query(sqlx::AssertSqlSafe(pipelines_sql))
            .bind(provider_id)
            .execute(p)
            .await
            .map(|_| ()));
    }

    pub async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        self.config_backend
            .get_table_preferences(provider_id, table_id)
            .await
    }

    pub async fn upsert_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()> {
        self.config_backend
            .set_table_preferences(provider_id, table_id, preferences_json)
            .await
    }

    pub async fn store_provider_permissions(
        &self, provider_id: i64, status: &PermissionStatus,
    ) -> DomainResult<()> {
        let stored_permissions = StoredPermissions {
            permissions: status
                .permissions
                .iter()
                .map(|check| (check.permission.name.clone(), check.granted))
                .collect(),
            last_checked: status.checked_at,
        };

        self.config_backend
            .store_permissions(provider_id, &stored_permissions)
            .await
    }
}
