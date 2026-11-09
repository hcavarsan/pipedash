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

    for migration in [
        include_str!("../../../../migrations/sqlite/20261020000002_vault_verifier.sql"),
        include_str!("../../../../migrations/sqlite/20261109000001_vault_kdf_salt.sql"),
    ] {
        sqlx::query(migration)
            .execute(&pool)
            .await
            .expect("Failed to create vault verifier table");
    }

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

async fn seed_legacy(pool: &Pool<Sqlite>, password: &str, with_verifier: bool) -> Vec<u8> {
    let legacy = Vault::legacy(password, &LEGACY_KDF).await.unwrap();
    let (nonce, ciphertext) = legacy.encrypt("secret").unwrap();
    sqlx::query("UPDATE providers SET encrypted_token = ?1, token_nonce = ?2 WHERE id = 1")
        .bind(&ciphertext)
        .bind(&nonce)
        .execute(pool)
        .await
        .unwrap();
    if with_verifier {
        let (nonce, verifier) = legacy.encrypt("pipedash-vault-verifier-v1").unwrap();
        sqlx::query("INSERT INTO vault_verifier (id, nonce, ciphertext) VALUES (1, ?1, ?2)")
            .bind(&nonce)
            .bind(&verifier)
            .execute(pool)
            .await
            .unwrap();
    }
    ciphertext
}

async fn stored_ciphertext(pool: &Pool<Sqlite>) -> Vec<u8> {
    sqlx::query_scalar("SELECT encrypted_token FROM providers WHERE id = 1")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn stored_salt(pool: &Pool<Sqlite>) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT kdf_salt FROM vault_verifier WHERE id = 1")
        .fetch_optional(pool)
        .await
        .unwrap()
        .flatten()
}

#[tokio::test]
async fn test_sqlite_wrong_password_does_not_claim_legacy_vault_without_verifier() {
    let pool = create_test_pool().await;
    let legacy_ciphertext = seed_legacy(&pool, "operator-password", false).await;

    let result = SqliteTokenStore::new(pool.clone(), Some("attacker-password".to_string())).await;

    assert!(result.is_err());
    assert_eq!(stored_ciphertext(&pool).await, legacy_ciphertext);
    assert_eq!(stored_salt(&pool).await, None);
    let store = SqliteTokenStore::new(pool, Some("operator-password".to_string()))
        .await
        .unwrap();
    assert_eq!(store.get_token(1).await.unwrap(), "secret");
}

#[tokio::test]
async fn test_sqlite_legacy_vault_moves_to_a_random_salt_and_keeps_tokens() {
    let pool = create_test_pool().await;
    let legacy_ciphertext = seed_legacy(&pool, "operator-password", true).await;

    let store = SqliteTokenStore::new(pool.clone(), Some("operator-password".to_string()))
        .await
        .unwrap();

    assert_eq!(store.get_token(1).await.unwrap(), "secret");
    assert_eq!(stored_salt(&pool).await.map(|salt| salt.len()), Some(16));
    assert_ne!(stored_ciphertext(&pool).await, legacy_ciphertext);
    let legacy = Vault::legacy("operator-password", &LEGACY_KDF)
        .await
        .unwrap();
    assert!(legacy
        .load(SqliteRows(&pool).tokens().await.unwrap())
        .await
        .is_err());
    let reopened = SqliteTokenStore::new(pool.clone(), Some("operator-password".to_string()))
        .await
        .unwrap();
    assert_eq!(reopened.get_token(1).await.unwrap(), "secret");
    let attacker = SqliteTokenStore::new(pool, Some("attacker-password".to_string())).await;
    assert!(matches!(
        attacker,
        Err(DomainError::AuthenticationFailed(_))
    ));
}

#[tokio::test]
async fn test_sqlite_wrong_password_leaves_legacy_vault_untouched() {
    let pool = create_test_pool().await;
    let legacy_ciphertext = seed_legacy(&pool, "operator-password", true).await;

    let result = SqliteTokenStore::new(pool.clone(), Some("attacker-password".to_string())).await;

    assert!(matches!(result, Err(DomainError::AuthenticationFailed(_))));
    assert_eq!(stored_ciphertext(&pool).await, legacy_ciphertext);
    assert_eq!(stored_salt(&pool).await, None);
}

#[tokio::test]
async fn test_sqlite_fresh_vaults_get_their_own_salt() {
    let first = create_test_pool().await;
    let second = create_test_pool().await;
    for pool in [&first, &second] {
        SqliteTokenStore::new(pool.clone(), Some("same-password".to_string()))
            .await
            .unwrap();
    }

    let (first_salt, second_salt) = (stored_salt(&first).await, stored_salt(&second).await);
    assert_eq!(first_salt.as_ref().map(Vec::len), Some(16));
    assert_ne!(first_salt, second_salt);
}

#[tokio::test]
async fn test_sqlite_token_written_by_an_older_version_after_upgrade_stays_readable() {
    let pool = create_test_pool().await;
    seed_legacy(&pool, "operator-password", true).await;
    SqliteTokenStore::new(pool.clone(), Some("operator-password".to_string()))
        .await
        .unwrap();
    seed_legacy(&pool, "operator-password", false).await;

    let store = SqliteTokenStore::new(pool, Some("operator-password".to_string()))
        .await
        .unwrap();

    assert_eq!(store.get_token(1).await.unwrap(), "secret");
}

#[tokio::test]
async fn test_sqlite_rekey_does_not_overwrite_a_vault_upgraded_by_another_process() {
    let pool = create_test_pool().await;
    let store = SqliteTokenStore::new(pool.clone(), Some("operator-password".to_string()))
        .await
        .unwrap();
    store.store_token(1, "secret").await.unwrap();
    let (salt, ciphertext) = (stored_salt(&pool).await, stored_ciphertext(&pool).await);
    let other = Vault::legacy("operator-password", &LEGACY_KDF)
        .await
        .unwrap();
    let (nonce, verifier) = other.encrypt("verifier").unwrap();

    let claimed = SqliteRows(&pool)
        .rekey(
            &store.vault,
            &other,
            VerifierRow {
                nonce,
                ciphertext: verifier,
                kdf_salt: Some(vec![0; 16]),
            },
        )
        .await
        .unwrap();

    assert!(!claimed);
    assert_eq!(stored_salt(&pool).await, salt);
    assert_eq!(stored_ciphertext(&pool).await, ciphertext);
}
