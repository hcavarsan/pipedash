use std::path::PathBuf;

use anyhow::{
    Context,
    Result,
};
use rusqlite::{
    Connection,
    OpenFlags,
};

const SCHEMA_VERSION: i32 = 3;

pub fn init_database(db_path: PathBuf) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("Failed to open database connection")?;

    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=10000;
         PRAGMA cache_size=-64000;
         PRAGMA temp_store=MEMORY;",
    )
    .context("Failed to set database pragmas")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        )",
        [],
    )
    .context("Failed to create schema_version table")?;

    let current_version: i32 = conn
        .query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current_version < SCHEMA_VERSION {
        migrate(&conn, current_version)?;
    }

    Ok(conn)
}

fn migrate(conn: &Connection, from_version: i32) -> Result<()> {
    if from_version < 1 {
        apply_migration_v1(conn)?;
    }

    if from_version < 2 {
        apply_migration_v2(conn)?;
    }

    if from_version < 3 {
        apply_migration_v3(conn)?;
    }

    conn.execute(
        "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
        [SCHEMA_VERSION],
    )
    .context("Failed to update schema version")?;

    Ok(())
}

fn apply_migration_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS providers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            provider_type TEXT NOT NULL,
            token_encrypted TEXT NOT NULL,
            config_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS pipelines_cache (
            id TEXT PRIMARY KEY,
            provider_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            status TEXT NOT NULL,
            repository TEXT NOT NULL,
            branch TEXT,
            workflow_file TEXT,
            last_run TEXT,
            last_updated TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_pipelines_provider ON pipelines_cache(provider_id);
        CREATE INDEX IF NOT EXISTS idx_pipelines_status ON pipelines_cache(status);
        ",
    )
    .context("Failed to apply migration v1")?;

    Ok(())
}

fn apply_migration_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS workflow_parameters_cache (
            workflow_id TEXT PRIMARY KEY,
            parameters_json TEXT NOT NULL,
            cached_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_workflow_params_cached_at ON workflow_parameters_cache(cached_at);
        ",
    )
    .context("Failed to apply migration v2")?;

    Ok(())
}

fn apply_migration_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        ALTER TABLE providers ADD COLUMN refresh_interval INTEGER NOT NULL DEFAULT 30;
        ",
    )
    .context("Failed to apply migration v3")?;

    Ok(())
}
