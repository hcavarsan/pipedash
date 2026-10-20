use std::sync::Arc;

use serde::Serialize;

use super::{
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
    target_token_store: Option<Arc<dyn TokenStore>>,
) -> DomainResult<ConfigAnalysis> {
    let new_config: PipedashConfig = toml::from_str(new_content)
        .map_err(|e| DomainError::InvalidConfig(format!("Invalid TOML syntax: {}", e)))?;

    let validation = new_config.validate();

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
    old.storage.backend != new.storage.backend || old.data_dir() != new.data_dir()
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
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::infrastructure::token_store::MemoryTokenStore;

    async fn sqlite_manager(data_dir: &TempDir) -> StorageManager {
        let mut config = PipedashConfig::default();
        config.storage.data_dir = data_dir.path().to_string_lossy().to_string();

        StorageManager::with_token_store(config, Arc::new(MemoryTokenStore::new()), true)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn switching_sqlite_to_postgres_returns_a_migration_plan() {
        let data_dir = TempDir::new().unwrap();
        let manager = sqlite_manager(&data_dir).await;
        let content = format!(
            "[storage]\ndata_dir = {:?}\nbackend = \"postgres\"\n\n[storage.postgres]\nconnection_string = \"postgres://user:pass@127.0.0.1:1/pipedash\"\n",
            data_dir.path().to_string_lossy()
        );

        let analysis = analyze_config(&content, &manager, None, None)
            .await
            .unwrap();

        let plan = analysis.migration_plan.expect("migration plan");
        assert_eq!(plan.to.storage.backend, StorageBackend::Postgres);
        assert!(!plan.steps.is_empty());
        assert!(analysis.stats.is_some());
        assert!(
            !analysis
                .postgres_connection
                .expect("connection test")
                .success
        );
    }

    #[tokio::test]
    async fn unchanged_storage_returns_no_migration_plan() {
        let data_dir = TempDir::new().unwrap();
        let manager = sqlite_manager(&data_dir).await;
        let content = format!(
            "[storage]\ndata_dir = {:?}\nbackend = \"sqlite\"\n",
            data_dir.path().to_string_lossy()
        );

        let analysis = analyze_config(&content, &manager, None, None)
            .await
            .unwrap();

        assert!(analysis.valid);
        assert!(analysis.migration_plan.is_none());
        assert!(analysis.postgres_connection.is_none());
    }

    #[tokio::test]
    async fn invalid_toml_is_an_invalid_config_error() {
        let data_dir = TempDir::new().unwrap();
        let manager = sqlite_manager(&data_dir).await;

        let err = analyze_config("[storage", &manager, None, None)
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::InvalidConfig(_)));
    }
}
