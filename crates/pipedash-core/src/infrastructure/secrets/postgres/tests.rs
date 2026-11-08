use super::*;

const TEST_LOCK_ID: i64 = 0x7069_7065_6461_7368;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests")
}

async fn create_test_pool() -> (Pool<Postgres>, [i64; 2]) {
    crate::infrastructure::database::init_postgres_database(&test_database_url())
        .await
        .expect("Failed to migrate test database")
        .close()
        .await;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&test_database_url())
        .await
        .expect("Failed to connect to test database");

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(TEST_LOCK_ID)
        .execute(&pool)
        .await
        .expect("Failed to take test lock");

    let mut providers = [0; 2];
    for (provider, name) in providers
        .iter_mut()
        .zip(["secrets-test-1", "secrets-test-2"])
    {
        *provider = sqlx::query_scalar(
            "INSERT INTO providers (name, provider_type, token_encrypted)
             VALUES ($1, 'test', '')
             ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
             RETURNING id",
        )
        .bind(name)
        .fetch_one(&pool)
        .await
        .expect("Failed to create test provider");
    }

    sqlx::query("TRUNCATE TABLE encrypted_tokens, vault_verifier")
        .execute(&pool)
        .await
        .expect("Failed to truncate test tables");

    (pool, providers)
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_postgres_basic() {
    let (pool, [first, _]) = create_test_pool().await;
    let connection_string = test_database_url();

    let store = PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
        .await
        .unwrap();

    store.store_token(first, "test-token").await.unwrap();
    let token = store.get_token(first).await.unwrap();
    assert_eq!(token, "test-token");

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_postgres_persistence() {
    let (pool, [first, _]) = create_test_pool().await;
    let connection_string = test_database_url();

    {
        let store = PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
            .await
            .unwrap();
        store.store_token(first, "persistent-token").await.unwrap();
    }

    {
        let store2 = PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
            .await
            .unwrap();
        let token = store2.get_token(first).await.unwrap();
        assert_eq!(token, "persistent-token");
    }

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_postgres_export_import() {
    let (pool, [first, second]) = create_test_pool().await;
    let connection_string = test_database_url();

    let store = PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
        .await
        .unwrap();
    store.store_token(first, "export-token").await.unwrap();
    store.store_token(second, "export-token-2").await.unwrap();

    let encrypted = store.export_encrypted("backup-password").await.unwrap();

    sqlx::query("TRUNCATE TABLE encrypted_tokens, vault_verifier")
        .execute(&pool)
        .await
        .unwrap();

    let store2 = PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
        .await
        .unwrap();

    store2
        .import_encrypted(&encrypted, "backup-password")
        .await
        .unwrap();

    assert_eq!(store2.get_token(first).await.unwrap(), "export-token");
    assert_eq!(store2.get_token(second).await.unwrap(), "export-token-2");

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_postgres_empty_vault_rejects_another_password() {
    let (pool, _) = create_test_pool().await;
    let connection_string = test_database_url();

    PostgresTokenStore::new(&connection_string, Some("operator-password".to_string()))
        .await
        .unwrap();

    let result =
        PostgresTokenStore::new(&connection_string, Some("attacker-password".to_string())).await;

    assert!(matches!(result, Err(DomainError::AuthenticationFailed(_))));
    PostgresTokenStore::new(&connection_string, Some("operator-password".to_string()))
        .await
        .unwrap();

    pool.close().await;
}
