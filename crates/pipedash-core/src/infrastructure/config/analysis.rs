use std::sync::Arc;

use pipedash_plugin_api::PluginRegistry;
use serde::Serialize;

use super::validation::ConfigValidator;
use super::{
    ConfigLoader,
    PipedashConfig,
    StorageBackend,
    StorageManager,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::event::EventBus;
use crate::infrastructure::migration::{
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
};
use crate::infrastructure::token_store::TokenStore;

#[derive(Debug, Clone, Serialize)]
pub struct ConfigIssue {
    pub field: String,
    pub message: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct ConnectionTestResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ConfigDataStats {
    pub providers_count: usize,
    pub tokens_count: usize,
    pub cache_entries_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ConfigAnalysis {
    pub valid: bool,
    pub errors: Vec<ConfigIssue>,
    pub warnings: Vec<ConfigIssue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_plan: Option<MigrationPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres_connection: Option<ConnectionTestResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ConfigDataStats>,
}

pub async fn analyze_config(
    new_content: &str, storage_manager: &StorageManager, event_bus: Option<Arc<dyn EventBus>>,
    target_token_store: Option<Arc<dyn TokenStore>>, registry: &PluginRegistry,
) -> DomainResult<ConfigAnalysis> {
    let content = new_content.to_owned();
    let new_config = tokio::task::spawn_blocking(move || ConfigLoader::parse(&content))
        .await
        .map_err(|e| DomainError::InternalError(format!("Config parse task failed: {e}")))?
        .map_err(|_| {
            DomainError::InvalidConfig(
                "Invalid config fields or environment references".to_string(),
            )
        })?;

    let validation = ConfigValidator::validate_with_plugins(&new_config, registry);

    let errors = validation
        .errors
        .iter()
        .map(|e| ConfigIssue {
            field: e.field.clone(),
            message: e.message.clone(),
            code: e.code.to_string(),
        })
        .collect();

    let warnings = validation
        .warnings
        .iter()
        .map(|w| ConfigIssue {
            field: w.field.clone(),
            message: w.message.clone(),
            code: w.code.to_string(),
        })
        .collect();

    let current_config = storage_manager.config();

    let (migration_plan, stats) = if backends_changed(current_config, &new_config) {
        let orchestrator =
            MigrationOrchestrator::from_manager(storage_manager, event_bus, target_token_store)
                .await;
        let plan = orchestrator.plan_migration(new_config.clone(), &MigrationOptions::default())?;

        let providers = storage_manager.config_backend().list_providers().await?;
        let tokens = storage_manager.token_store().await.get_all_tokens().await?;
        let cache_entries = storage_manager.cache_backend().list(None).await?;

        let stats = ConfigDataStats {
            providers_count: providers.len(),
            tokens_count: tokens.len(),
            cache_entries_count: cache_entries.len(),
        };

        (Some(plan), Some(stats))
    } else {
        (None, None)
    };

    let postgres_connection = if should_test_postgres_connection(current_config, &new_config) {
        Some(test_postgres_connection(&new_config.storage.postgres.connection_string).await)
    } else {
        None
    };

    Ok(ConfigAnalysis {
        valid: validation.is_ok(),
        errors,
        warnings,
        migration_plan,
        postgres_connection,
        stats,
    })
}

fn backends_changed(old: &PipedashConfig, new: &PipedashConfig) -> bool {
    !crate::infrastructure::migration::remap::same_database(old, new)
        || old.data_dir() != new.data_dir()
}

fn should_test_postgres_connection(old: &PipedashConfig, new: &PipedashConfig) -> bool {
    let uses_postgres = new.storage.backend == StorageBackend::Postgres;

    let connection_changed =
        old.storage.postgres.connection_string != new.storage.postgres.connection_string;

    let switching_to_postgres = old.storage.backend != StorageBackend::Postgres
        && new.storage.backend == StorageBackend::Postgres;

    uses_postgres && (connection_changed || switching_to_postgres)
}

async fn test_postgres_connection(connection_string: &str) -> ConnectionTestResult {
    #[cfg(feature = "postgres")]
    {
        use std::time::Instant;

        use sqlx::postgres::PgPoolOptions;

        if connection_string.is_empty() {
            return ConnectionTestResult {
                success: false,
                message: "PostgreSQL connection string is empty".to_string(),
                latency_ms: None,
            };
        }

        let start = Instant::now();

        let timeout_duration = std::time::Duration::from_secs(5);
        let connect_result = tokio::time::timeout(timeout_duration, async {
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(std::time::Duration::from_secs(3))
                .connect(connection_string)
                .await?;

            sqlx::query("SELECT 1").execute(&pool).await?;

            Ok::<_, sqlx::Error>(pool)
        })
        .await;

        match connect_result {
            Ok(Ok(_pool)) => {
                let latency = start.elapsed().as_millis() as u64;
                ConnectionTestResult {
                    success: true,
                    message: "Successfully connected to PostgreSQL".to_string(),
                    latency_ms: Some(latency),
                }
            }
            Ok(Err(e)) => ConnectionTestResult {
                success: false,
                message: format!("Failed to connect to PostgreSQL: {}", e),
                latency_ms: None,
            },
            Err(_) => ConnectionTestResult {
                success: false,
                message: format!(
                    "Connection timeout after {} seconds",
                    timeout_duration.as_secs()
                ),
                latency_ms: None,
            },
        }
    }

    #[cfg(not(feature = "postgres"))]
    {
        let _ = connection_string;
        ConnectionTestResult {
            success: false,
            message: "PostgreSQL feature not enabled. Compile with --features postgres".to_string(),
            latency_ms: None,
        }
    }
}

#[cfg(test)]
mod tests;
