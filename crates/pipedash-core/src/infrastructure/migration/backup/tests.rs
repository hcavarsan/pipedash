use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{
    Connection,
    SqliteConnection,
};

use super::{
    BackupManager,
    BackupPaths,
};
use crate::infrastructure::config::PipedashConfig;

async fn check_wal_backup(target: bool) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.path().to_str().unwrap().to_owned();
    let pool = crate::infrastructure::database::init_database(config.db_path())
        .await
        .unwrap();
    let mut writer = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint = 0")
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO providers (id, name, provider_type, token_encrypted) VALUES (42, 'committed', 'stub', '')",
    )
    .execute(&mut *writer)
    .await
    .unwrap();
    let mut transaction = writer.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO providers (name, provider_type, token_encrypted) VALUES ('uncommitted', 'stub', '')",
    )
    .execute(&mut *transaction)
    .await
    .unwrap();

    let manager = BackupManager::new(dir.path().to_path_buf());
    let backup = if target {
        manager.backup_target_database(&config, "wal").await
    } else {
        manager.backup_source_database(&config, None, "wal").await
    }
    .unwrap()
    .unwrap();
    let standalone = tempfile::tempdir().unwrap();
    let snapshot_path = standalone.path().join("snapshot.db");
    tokio::fs::copy(backup, &snapshot_path).await.unwrap();
    let mut snapshot = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(snapshot_path)
            .read_only(true),
    )
    .await
    .unwrap();
    let providers: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM providers ORDER BY id")
            .fetch_all(&mut snapshot)
            .await
            .unwrap();
    assert_eq!(providers, vec![(42, "committed".to_owned())]);
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut snapshot)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    snapshot.close().await.unwrap();
    transaction.rollback().await.unwrap();
    drop(writer);
    pool.close().await;
}

#[tokio::test]
async fn source_backup_includes_committed_wal_providers() {
    check_wal_backup(false).await;
}

#[tokio::test]
async fn target_backup_includes_committed_wal_providers() {
    check_wal_backup(true).await;
}

#[tokio::test]
async fn restore_keeps_open_wal_source_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.path().to_str().unwrap().to_owned();
    let pool = crate::infrastructure::database::init_database(config.db_path())
        .await
        .unwrap();
    let mut writer = pool.acquire().await.unwrap();
    for statement in [
        "PRAGMA wal_autocheckpoint = 0",
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 200) \
         INSERT INTO providers (name, provider_type, token_encrypted) \
         SELECT 'bulk' || i, 'stub', hex(randomblob(1000)) FROM n",
        "PRAGMA wal_checkpoint(TRUNCATE)",
        "DELETE FROM providers",
        "INSERT INTO providers (id, name, provider_type, token_encrypted) VALUES (42, 'kept', 'stub', '')",
    ] {
        sqlx::query(statement).execute(&mut *writer).await.unwrap();
    }

    let manager = BackupManager::new(dir.path().to_path_buf());
    let source_db = manager
        .backup_source_database(&config, None, "restore")
        .await
        .unwrap();
    sqlx::query("INSERT INTO providers (name, provider_type, token_encrypted) VALUES ('failed', 'stub', '')")
        .execute(&mut *writer)
        .await
        .unwrap();
    let backups = BackupPaths {
        source_db,
        source_config: dir.path().join("missing-config.toml.backup"),
        target_db: None,
        timestamp: "restore".into(),
    };
    manager.restore_backups(&backups).await.unwrap();

    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    let providers: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM providers ORDER BY id")
            .fetch_all(&mut *writer)
            .await
            .unwrap();
    assert_eq!(providers, vec![(42, "kept".to_owned())]);
    drop(writer);
    pool.close().await;
}

async fn check_target_restore(postgres_source: bool) {
    let source_dir = tempfile::tempdir().unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let mut target_config = PipedashConfig::default();
    target_config.storage.data_dir = target_dir.path().to_str().unwrap().to_owned();
    let pool = crate::infrastructure::database::init_database(target_config.db_path())
        .await
        .unwrap();
    sqlx::query("INSERT INTO providers (id, name, provider_type, token_encrypted) VALUES (7, 'original', 'stub', '')")
        .execute(&pool)
        .await
        .unwrap();

    let manager = BackupManager::new(source_dir.path().to_path_buf());
    let target_db = manager
        .backup_target_database(&target_config, "target")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO providers (name, provider_type, token_encrypted) VALUES ('first', 'stub', '')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let source_db = if postgres_source {
        let json_backup = source_dir.path().join("postgres_backup_target.json");
        tokio::fs::write(&json_backup, "{\"providers\": []}")
            .await
            .unwrap();
        Some(json_backup)
    } else {
        None
    };
    let backups = BackupPaths {
        source_db,
        source_config: source_dir.path().join("missing-config.toml.backup"),
        target_db,
        timestamp: "target".into(),
    };
    manager.restore_backups(&backups).await.unwrap();

    let providers: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM providers ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(providers, vec![(7, "original".to_owned())]);
    pool.close().await;
}

#[tokio::test]
async fn restore_drops_committed_target_writes() {
    check_target_restore(false).await;
}

#[tokio::test]
async fn restore_drops_committed_target_writes_after_postgres_source() {
    check_target_restore(true).await;
}

#[tokio::test]
async fn restore_keeps_sqlite_file_after_postgres_backup() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::env::set_var("PIPEDASH_CONFIG_PATH", &config_path);
    tokio::fs::write(&config_path, "[storage]\nbackend = \"postgres\"\n")
        .await
        .unwrap();
    let config_backup = dir.path().join("config.toml.backup-x");
    tokio::fs::copy(&config_path, &config_backup).await.unwrap();
    let sqlite_path = dir.path().join("pipedash.db");
    tokio::fs::write(&sqlite_path, b"SQLite format 3\0")
        .await
        .unwrap();
    let json_backup = dir.path().join("postgres_backup_x.json");
    tokio::fs::write(&json_backup, "{\"providers\": []}")
        .await
        .unwrap();
    let backups = BackupPaths {
        source_db: Some(json_backup),
        source_config: config_backup,
        target_db: None,
        timestamp: "x".into(),
    };

    BackupManager::new(dir.path().to_path_buf())
        .restore_backups(&backups)
        .await
        .unwrap();

    let after = tokio::fs::read(&sqlite_path).await.unwrap();
    assert_eq!(
        after,
        b"SQLite format 3\0",
        "{}",
        String::from_utf8_lossy(&after)
    );
}
