use std::path::PathBuf;
use std::time::Duration;

use anyhow::{
    Context,
    Result,
};
use sqlx::{
    sqlite::{
        SqliteConnectOptions,
        SqliteJournalMode,
        SqlitePoolOptions,
        SqliteSynchronous,
    },
    SqlitePool,
};

pub async fn init_metrics_database(db_path: PathBuf) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(options)
        .await
        .context("Failed to create metrics database pool")?;

    sqlx::query("PRAGMA cache_size=-64000")
        .execute(&pool)
        .await
        .context("Failed to set cache_size")?;

    sqlx::query("PRAGMA temp_store=MEMORY")
        .execute(&pool)
        .await
        .context("Failed to set temp_store")?;

    sqlx::query("PRAGMA wal_autocheckpoint=1000")
        .execute(&pool)
        .await
        .context("Failed to set wal_autocheckpoint")?;

    mark_old_metrics_migrations_as_applied(&pool).await?;

    sqlx::migrate!("./migrations_metrics")
        .run(&pool)
        .await
        .context("Failed to run metrics migrations")?;

    Ok(pool)
}

async fn mark_old_metrics_migrations_as_applied(_pool: &SqlitePool) -> Result<()> {
    Ok(())
}
