use std::path::PathBuf;
use std::time::Duration;

use sqlx::postgres::{
    PgPool,
    PgPoolOptions,
};
use sqlx::sqlite::{
    SqliteConnectOptions,
    SqlitePoolOptions,
};
use sqlx::{
    Executor,
    SqlitePool,
};

pub async fn init_database(path: PathBuf) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5)); // Reduced from 30s to 5s to prevent compounding with retries

    let pool = SqlitePoolOptions::new()
        .max_connections(50)
        .min_connections(10)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations/sqlite")
        .set_ignore_missing(true)
        .run(&pool)
        .await?;

    pool.execute("PRAGMA cache_size = -64000").await?; // 64MB cache
    pool.execute("PRAGMA temp_store = MEMORY").await?;
    pool.execute("PRAGMA mmap_size = 268435456").await?; // 256MB mmap
    pool.execute("PRAGMA auto_vacuum = INCREMENTAL").await?; // Enables incremental auto-vacuum
    pool.execute("PRAGMA wal_autocheckpoint = 1000").await?; // Checkpoint every 1000 pages

    Ok(pool)
}

pub async fn init_postgres_database(connection_string: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .min_connections(10)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO public")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(connection_string)
        .await?;

    sqlx::migrate!("./migrations/postgres")
        .set_ignore_missing(true)
        .run(&pool)
        .await?;

    Ok(pool)
}
