use std::path::Path;

mod metrics_repository;
mod repository;
mod schema;
mod sqlite_backend;

#[cfg(feature = "postgres")]
mod postgres_backend;

pub use metrics_repository::MetricsRepository;
#[cfg(feature = "postgres")]
pub use postgres_backend::PostgresConfigBackend;
pub use repository::{
    DatabasePool,
    Repository,
};
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
