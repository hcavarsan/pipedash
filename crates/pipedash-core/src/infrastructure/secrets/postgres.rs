use std::collections::HashMap;
use std::sync::Arc;

use aes_gcm::{
    aead::{
        Aead,
        KeyInit,
    },
    Aes256Gcm,
    Nonce,
};
use async_trait::async_trait;
use sqlx::{
    Pool,
    Postgres,
};
use tokio::sync::RwLock;

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::TokenStore;

const BACKUP_KDF_SALT: &[u8] = b"pipedash-backup-salt-v1";

pub struct PostgresTokenStore {
    pool: Pool<Postgres>,
    encryption_key: [u8; 32],
    cache: Arc<RwLock<HashMap<i64, String>>>,
}

impl PostgresTokenStore {
    pub async fn new(
        connection_string: &str, vault_password: Option<String>,
    ) -> DomainResult<Self> {
        use sqlx::postgres::PgPoolOptions;

        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("SET search_path TO public")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(connection_string)
            .await
            .map_err(|e| {
                DomainError::DatabaseError(format!("Failed to connect to PostgreSQL: {}", e))
            })?;

        let password = vault_password
            .or_else(|| std::env::var("PIPEDASH_VAULT_PASSWORD").ok())
            .unwrap_or_else(Self::generate_random_key);

        let encryption_key = Self::derive_encryption_key(&password);

        let store = Self {
            pool,
            encryption_key,
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        store.load_to_cache().await?;

        tracing::info!("PostgreSQL token store initialized");

        Ok(store)
    }

    fn generate_random_key() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::rng();
        (0..64)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    fn derive_encryption_key(password: &str) -> [u8; 32] {
        use argon2::{
            Argon2,
            ParamsBuilder,
        };

        let salt = b"pipedash-postgres-salt-v1";
        let mut output = [0u8; 32];

        let params = ParamsBuilder::new()
            .m_cost(4096)
            .t_cost(1)
            .p_cost(1)
            .output_len(32)
            .build()
            .expect("Invalid Argon2 parameters");

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        argon2
            .hash_password_into(password.as_bytes(), salt, &mut output)
            .expect("Failed to derive encryption key");

        output
    }

    async fn encrypt_token(&self, plaintext: &str) -> DomainResult<(Vec<u8>, Vec<u8>)> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {}", e)))?;

        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| DomainError::InternalError(format!("Encryption failed: {}", e)))?;

        Ok((nonce_bytes.to_vec(), ciphertext))
    }

    async fn decrypt_token(&self, nonce: &[u8], ciphertext: &[u8]) -> DomainResult<String> {
        if nonce.len() != 12 {
            return Err(DomainError::InvalidConfig("Invalid nonce length".into()));
        }

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {}", e)))?;

        let nonce_array: [u8; 12] = nonce
            .try_into()
            .map_err(|_| DomainError::InvalidConfig("Invalid nonce".into()))?;
        let nonce = Nonce::from(nonce_array);

        let plaintext = cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| DomainError::AuthenticationFailed("Decryption failed".into()))?;

        String::from_utf8(plaintext)
            .map_err(|e| DomainError::DatabaseError(format!("Invalid UTF-8: {}", e)))
    }

    async fn load_to_cache(&self) -> DomainResult<()> {
        let rows = sqlx::query_as::<_, (i64, Vec<u8>, Vec<u8>)>(
            "SELECT provider_id, nonce, ciphertext FROM encrypted_tokens",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to load tokens: {}", e)))?;

        let mut cache = self.cache.write().await;
        cache.clear();

        let mut decryption_errors = Vec::new();

        for (provider_id, nonce, ciphertext) in rows {
            match self.decrypt_token(&nonce, &ciphertext).await {
                Ok(token) => {
                    cache.insert(provider_id, token);
                }
                Err(e) => {
                    tracing::warn!(
                        provider_id = provider_id,
                        error = %e,
                        "Failed to decrypt token for provider"
                    );
                    decryption_errors.push((provider_id, e));
                }
            }
        }

        if !decryption_errors.is_empty() {
            let failed_count = decryption_errors.len();
            return Err(DomainError::AuthenticationFailed(format!(
                "Failed to decrypt {} token(s) - wrong vault password",
                failed_count
            )));
        }

        tracing::debug!(count = cache.len(), "Loaded tokens to cache");
        Ok(())
    }
}

#[async_trait]
impl TokenStore for PostgresTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let (nonce, ciphertext) = self.encrypt_token(token).await?;

        sqlx::query(
            "INSERT INTO encrypted_tokens (provider_id, nonce, ciphertext, created_at, updated_at)
             VALUES ($1, $2, $3, NOW(), NOW())
             ON CONFLICT (provider_id)
             DO UPDATE SET nonce = $2, ciphertext = $3, updated_at = NOW()",
        )
        .bind(provider_id)
        .bind(&nonce)
        .bind(&ciphertext)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to store token: {}", e)))?;

        let mut cache = self.cache.write().await;
        cache.insert(provider_id, token.to_string());

        Ok(())
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        {
            let cache = self.cache.read().await;
            if let Some(token) = cache.get(&provider_id) {
                return Ok(token.clone());
            }
        }

        let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
            "SELECT nonce, ciphertext FROM encrypted_tokens WHERE provider_id = $1",
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to get token: {}", e)))?
        .ok_or_else(|| {
            DomainError::DatabaseError(format!("Token not found for provider {}", provider_id))
        })?;

        let token = self.decrypt_token(&row.0, &row.1).await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(provider_id, token.clone());
        }

        Ok(token)
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        sqlx::query("DELETE FROM encrypted_tokens WHERE provider_id = $1")
            .bind(provider_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to delete token: {}", e)))?;

        let mut cache = self.cache.write().await;
        cache.remove(&provider_id);

        Ok(())
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        let cache = self.cache.read().await;
        Ok(cache.clone())
    }

    async fn export_encrypted(&self, password: &str) -> DomainResult<Vec<u8>> {
        let tokens = self.get_all_tokens().await?;
        let json = serde_json::to_vec(&tokens).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to serialize tokens: {}", e))
        })?;

        let backup_key = Self::derive_backup_key(password);
        let cipher = Aes256Gcm::new_from_slice(&backup_key)
            .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {}", e)))?;

        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = cipher
            .encrypt(&nonce, json.as_ref())
            .map_err(|e| DomainError::InternalError(format!("Encryption failed: {}", e)))?;

        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        Ok(result)
    }

    async fn import_encrypted(&self, data: &[u8], password: &str) -> DomainResult<()> {
        if data.len() < 12 {
            return Err(DomainError::InvalidConfig("Invalid backup data".into()));
        }

        let nonce_bytes: [u8; 12] = data[..12]
            .try_into()
            .map_err(|_| DomainError::InvalidConfig("Invalid nonce".into()))?;
        let ciphertext = &data[12..];

        let backup_key = Self::derive_backup_key(password);
        let cipher = Aes256Gcm::new_from_slice(&backup_key)
            .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {}", e)))?;

        let nonce = Nonce::from(nonce_bytes);

        let plaintext = cipher.decrypt(&nonce, ciphertext).map_err(|_| {
            DomainError::AuthenticationFailed("Decryption failed (wrong password?)".into())
        })?;

        let tokens: HashMap<i64, String> = serde_json::from_slice(&plaintext)
            .map_err(|e| DomainError::DatabaseError(format!("Failed to parse tokens: {}", e)))?;

        for (provider_id, token) in tokens {
            self.store_token(provider_id, &token).await?;
        }

        Ok(())
    }
}

impl PostgresTokenStore {
    fn derive_backup_key(password: &str) -> [u8; 32] {
        use argon2::{
            Argon2,
            ParamsBuilder,
        };

        let mut output = [0u8; 32];

        let params = ParamsBuilder::new()
            .m_cost(4096)
            .t_cost(1)
            .p_cost(1)
            .output_len(32)
            .build()
            .expect("Invalid Argon2 parameters");

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        argon2
            .hash_password_into(password.as_bytes(), BACKUP_KDF_SALT, &mut output)
            .expect("Failed to derive backup key");

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_pool() -> Pool<Postgres> {
        let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@localhost:5432/pipedash_test".to_string()
        });

        let pool = Pool::<Postgres>::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS encrypted_tokens (
                provider_id BIGINT PRIMARY KEY,
                nonce BYTEA NOT NULL,
                ciphertext BYTEA NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create test table");

        sqlx::query("TRUNCATE TABLE encrypted_tokens")
            .execute(&pool)
            .await
            .expect("Failed to truncate test table");

        pool
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_basic() {
        let pool = create_test_pool().await;
        let connection_string = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@localhost:5432/pipedash_test".to_string()
        });

        let store = PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
            .await
            .unwrap();

        store.store_token(1, "test-token").await.unwrap();
        let token = store.get_token(1).await.unwrap();
        assert_eq!(token, "test-token");

        pool.close().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_persistence() {
        let pool = create_test_pool().await;
        let connection_string = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@localhost:5432/pipedash_test".to_string()
        });

        {
            let store =
                PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
                    .await
                    .unwrap();
            store.store_token(1, "persistent-token").await.unwrap();
        }

        {
            let store2 =
                PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
                    .await
                    .unwrap();
            let token = store2.get_token(1).await.unwrap();
            assert_eq!(token, "persistent-token");
        }

        pool.close().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_export_import() {
        let pool = create_test_pool().await;
        let connection_string = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@localhost:5432/pipedash_test".to_string()
        });

        let store = PostgresTokenStore::new(&connection_string, Some("test-password".to_string()))
            .await
            .unwrap();
        store.store_token(1, "export-token").await.unwrap();
        store.store_token(2, "export-token-2").await.unwrap();

        let encrypted = store.export_encrypted("backup-password").await.unwrap();

        sqlx::query("TRUNCATE TABLE encrypted_tokens")
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

        assert_eq!(store2.get_token(1).await.unwrap(), "export-token");
        assert_eq!(store2.get_token(2).await.unwrap(), "export-token-2");

        pool.close().await;
    }
}
