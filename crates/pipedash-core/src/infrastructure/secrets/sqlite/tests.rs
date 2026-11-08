use super::*;

async fn create_test_pool() -> Pool<Sqlite> {
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("Failed to create in-memory pool");

    sqlx::query(
        "CREATE TABLE providers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            provider_type TEXT NOT NULL,
            token_encrypted TEXT NOT NULL DEFAULT '',
            config_json TEXT NOT NULL DEFAULT '{}',
            encrypted_token BLOB,
            token_nonce BLOB,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(&pool)
    .await
    .expect("Failed to create table");

    sqlx::query(include_str!(
        "../../../../migrations/sqlite/20261020000002_vault_verifier.sql"
    ))
    .execute(&pool)
    .await
    .expect("Failed to create vault verifier table");

    sqlx::query("INSERT INTO providers (name, provider_type) VALUES ('test-provider', 'stub')")
        .execute(&pool)
        .await
        .expect("Failed to insert provider");

    pool
}

#[tokio::test]
async fn test_sqlite_basic() {
    let pool = create_test_pool().await;
    let store = SqliteTokenStore::new(pool, Some("test-password".to_string()))
        .await
        .unwrap();

    store.store_token(1, "test-token").await.unwrap();
    let token = store.get_token(1).await.unwrap();
    assert_eq!(token, "test-token");
}

#[tokio::test]
async fn test_sqlite_cache() {
    let pool = create_test_pool().await;
    let store = SqliteTokenStore::new(pool, Some("test-password".to_string()))
        .await
        .unwrap();

    store.store_token(1, "cached-token").await.unwrap();

    let token = store.get_token(1).await.unwrap();
    assert_eq!(token, "cached-token");
}

#[tokio::test]
async fn test_sqlite_delete() {
    let pool = create_test_pool().await;
    let store = SqliteTokenStore::new(pool, Some("test-password".to_string()))
        .await
        .unwrap();

    store.store_token(1, "to-delete").await.unwrap();
    store.delete_token(1).await.unwrap();

    assert!(store.get_token(1).await.is_err());
}

#[tokio::test]
async fn test_sqlite_export_import() {
    let pool = create_test_pool().await;
    let store = SqliteTokenStore::new(pool.clone(), Some("test-password".to_string()))
        .await
        .unwrap();

    store.store_token(1, "export-token").await.unwrap();

    let encrypted = store.export_encrypted("backup-password").await.unwrap();

    store.delete_token(1).await.unwrap();

    store
        .import_encrypted(&encrypted, "backup-password")
        .await
        .unwrap();

    let token = store.get_token(1).await.unwrap();
    assert_eq!(token, "export-token");
}

#[tokio::test]
async fn test_sqlite_wrong_backup_password() {
    let pool = create_test_pool().await;
    let store = SqliteTokenStore::new(pool, Some("test-password".to_string()))
        .await
        .unwrap();

    store.store_token(1, "secret").await.unwrap();
    let encrypted = store.export_encrypted("correct-password").await.unwrap();

    let result = store.import_encrypted(&encrypted, "wrong-password").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_sqlite_empty_vault_rejects_another_password() {
    let pool = create_test_pool().await;
    SqliteTokenStore::new(pool.clone(), Some("operator-password".to_string()))
        .await
        .unwrap();

    let result = SqliteTokenStore::new(pool.clone(), Some("attacker-password".to_string())).await;

    assert!(matches!(result, Err(DomainError::AuthenticationFailed(_))));
    SqliteTokenStore::new(pool, Some("operator-password".to_string()))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_sqlite_wrong_password_does_not_claim_vault_without_verifier() {
    let pool = create_test_pool().await;
    let store = SqliteTokenStore::new(pool.clone(), Some("operator-password".to_string()))
        .await
        .unwrap();
    store.store_token(1, "secret").await.unwrap();
    sqlx::query("DELETE FROM vault_verifier")
        .execute(&pool)
        .await
        .unwrap();

    let result = SqliteTokenStore::new(pool.clone(), Some("attacker-password".to_string())).await;

    assert!(result.is_err());
    let store = SqliteTokenStore::new(pool, Some("operator-password".to_string()))
        .await
        .unwrap();
    assert_eq!(store.get_token(1).await.unwrap(), "secret");
}
