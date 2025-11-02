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

pub async fn init_database(db_path: PathBuf) -> Result<SqlitePool> {
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
        .context("Failed to create database pool")?;

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

    mark_old_migrations_as_applied(&pool).await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to run migrations")?;

    Ok(pool)
}

async fn mark_old_migrations_as_applied(pool: &SqlitePool) -> Result<()> {
    let has_old_schema: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
    )
    .fetch_one(pool)
    .await?;

    if !has_old_schema {
        return Ok(());
    }

    let old_version: i32 =
        sqlx::query_scalar("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
            .fetch_optional(pool)
            .await?
            .unwrap_or(0);

    if old_version == 0 {
        return Ok(());
    }

    let has_sqlx_migrations: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;

    if has_sqlx_migrations {
        return Ok(());
    }

    eprintln!(
        "[MIGRATION] Detected old rusqlite schema (version {}), marking sqlx migrations as applied",
        old_version
    );

    sqlx::query(
        "CREATE TABLE _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let migrations: Vec<(i64, &str, &[u8])> = vec![
        (
            20250101000001i64,
            "initial_schema",
            include_bytes!("../../../migrations/20250101000001_initial_schema.sql"),
        ),
        (
            20250101000002i64,
            "workflow_parameters",
            include_bytes!("../../../migrations/20250101000002_workflow_parameters.sql"),
        ),
        (
            20250101000003i64,
            "refresh_interval",
            include_bytes!("../../../migrations/20250101000003_refresh_interval.sql"),
        ),
        (
            20250101000004i64,
            "run_history",
            include_bytes!("../../../migrations/20250101000004_run_history.sql"),
        ),
    ];

    for (version_num, description, sql_bytes) in migrations.iter().take(old_version as usize) {
        use sha2::{
            Digest,
            Sha384,
        };
        let mut hasher = Sha384::new();
        hasher.update(sql_bytes);
        let checksum = hasher.finalize();

        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, ?, 1, ?, 0)",
        )
        .bind(version_num)
        .bind(description)
        .bind(&checksum[..])
        .execute(pool)
        .await?;
    }

    eprintln!(
        "[MIGRATION] Marked {} migrations as applied from old schema",
        old_version
    );

    Ok(())
}
