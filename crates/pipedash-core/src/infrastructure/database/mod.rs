use std::path::Path;

macro_rules! on_pool {
    ($pool:expr, $p:ident => $body:expr) => {
        match $pool {
            DatabasePool::Sqlite($p) => $body,
            DatabasePool::Postgres($p) => $body,
        }
    };
}

mod bind;
mod config_transfer;
mod correlation_repository;
mod device_alert_repository;
mod device_push_repository;
mod device_repository;
pub(crate) mod metrics_repository;
mod pipeline_view_repository;
mod pool;
mod provider_row;
mod push_repository;
mod repository;
mod retry;
mod schema;
mod sqlite_backend;

#[cfg(feature = "postgres")]
mod postgres_backend;

pub use correlation_repository::{
    ApplicationMemberRow,
    ApplicationOverrideRow,
    ApplicationRow,
    ApplicationTargetRow,
    ArtifactObservationRow,
    ArtifactRow,
    ChangeRow,
    ChangeStageRow,
    CorrelationConn,
    CorrelationEdgeRow,
    CorrelationRepository,
    LogScanRow,
    ObservationWindowRow,
    ObservedTransitionRow,
    RunEvidenceRow,
    SourceObservationRow,
    SubjectIdentityRow,
    TargetDirectionRow,
    TargetObservationRow,
    PIPELINE_LEVEL,
};
pub use device_alert_repository::DeviceAlertRepository;
pub use device_push_repository::DevicePushRepository;
pub use device_repository::DeviceRepository;
pub use metrics_repository::{
    MetricsRepository,
    RecordMetrics,
};
pub use pipeline_view_repository::{
    PipelineViewRepository,
    StoredPin,
    StoredView,
    ViewExport,
};
pub use pool::DatabasePool;
#[cfg(feature = "postgres")]
pub use postgres_backend::PostgresConfigBackend;
pub use push_repository::PushRepository;
pub use repository::fetch_status::ProviderFetchStatus;
pub(crate) use repository::fetch_status::{
    FETCH_STATUS_ERROR,
    FETCH_STATUS_NEVER,
    FETCH_STATUS_SUCCESS,
};
pub use repository::Repository;
pub use schema::init_database;
#[cfg(feature = "postgres")]
pub use schema::init_postgres_database;
pub use sqlite_backend::SqliteConfigBackend;

pub async fn has_encrypted_tokens(db_path: &Path) -> bool {
    if !db_path.exists() {
        return false;
    }

    let db_url = format!("sqlite:{}?mode=ro", db_path.display());

    let pool = match sqlx::SqlitePool::connect(&db_url).await {
        Ok(pool) => pool,
        Err(e) => {
            tracing::debug!(
                "Could not connect to database to check encrypted tokens: {}",
                e
            );
            return false;
        }
    };

    let result: Result<i64, _> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM providers WHERE encrypted_token IS NOT NULL AND token_nonce IS NOT NULL"
    )
    .fetch_one(&pool)
    .await;

    let has_tokens = result.map(|count| count > 0).unwrap_or(false);

    tracing::debug!(
        "Checked for encrypted tokens in {}: {}",
        db_path.display(),
        if has_tokens { "found" } else { "none" }
    );

    has_tokens
}
