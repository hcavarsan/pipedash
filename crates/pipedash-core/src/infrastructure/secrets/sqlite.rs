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
    Sqlite,
};
use tokio::sync::RwLock;

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::TokenStore;

const SQLITE_KDF_SALT: &[u8] = b"pipedash-sqlite-vault-v1";
const BACKUP_KDF_SALT: &[u8] = b"pipedash-backup-salt-v1";

pub struct SqliteTokenStore {
    pool: Pool<Sqlite>,
    encryption_key: [u8; 32],
    cache: Arc<RwLock<HashMap<i64, String>>>,
}

impl SqliteTokenStore {
    pub async fn new(pool: Pool<Sqlite>, vault_password: Option<String>) -> DomainResult<Self> {
        let password = vault_password
            .or_else(|| std::env::var("PIPEDASH_VAULT_PASSWORD").ok())
            .ok_or_else(|| {
                DomainError::InvalidConfig(
                    "PIPEDASH_VAULT_PASSWORD environment variable required for SQLite token encryption".into(),
                )
            })?;

        let encryption_key = Self::derive_encryption_key(&password);

        let store = Self {
            pool,
            encryption_key,
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        store.load_to_cache().await?;

        tracing::info!("SQLite token store initialized");
        Ok(store)
    }

    pub async fn with_pool(
        pool: Pool<Sqlite>, vault_password: Option<String>,
    ) -> DomainResult<Self> {
        Self::new(pool, vault_password).await
    }

    fn derive_encryption_key(password: &str) -> [u8; 32] {
        use argon2::{
            Argon2,
            ParamsBuilder,
        };

        let mut output = [0u8; 32];

        let params = ParamsBuilder::new()
            .m_cost(65536)
            .t_cost(3)
            .p_cost(1)
            .output_len(32)
            .build()
            .expect("Invalid Argon2 parameters");

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        argon2
            .hash_password_into(password.as_bytes(), SQLITE_KDF_SALT, &mut output)
            .expect("Failed to derive encryption key");

        output
    }

    fn derive_backup_key(password: &str) -> [u8; 32] {
        use argon2::{
            Argon2,
            ParamsBuilder,
        };

        let mut output = [0u8; 32];

        let params = ParamsBuilder::new()
            .m_cost(65536)
            .t_cost(3)
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

        let plaintext = cipher.decrypt(&nonce, ciphertext).map_err(|_| {
            DomainError::AuthenticationFailed("Token decryption failed - wrong password?".into())
        })?;

        String::from_utf8(plaintext)
            .map_err(|e| DomainError::DatabaseError(format!("Invalid UTF-8 in token: {}", e)))
    }

    async fn load_to_cache(&self) -> DomainResult<()> {
        let rows = sqlx::query_as::<_, (i64, Vec<u8>, Vec<u8>)>(
            "SELECT id, encrypted_token, token_nonce FROM providers
             WHERE encrypted_token IS NOT NULL AND token_nonce IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to load tokens: {}", e)))?;

        let mut cache = self.cache.write().await;
        cache.clear();

        let mut decryption_errors = Vec::new();

        for (provider_id, ciphertext, nonce) in rows {
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
impl TokenStore for SqliteTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        tracing::debug!(
            "[SqliteTokenStore] Storing token for provider {} (token length: {})",
            provider_id,
            token.len()
        );

        let (nonce, ciphertext) = self.encrypt_token(token).await?;
        tracing::debug!(
            "[SqliteTokenStore] Token encrypted: nonce={} bytes, ciphertext={} bytes",
            nonce.len(),
            ciphertext.len()
        );

        let result = sqlx::query(
            "UPDATE providers SET encrypted_token = ?1, token_nonce = ?2, updated_at = datetime('now')
             WHERE id = ?3",
        )
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(provider_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to store token: {}", e)))?;

        tracing::info!(
            "[SqliteTokenStore] Token stored for provider {} (rows affected: {})",
            provider_id,
            result.rows_affected()
        );

        if result.rows_affected() == 0 {
            tracing::warn!(
                "[SqliteTokenStore] No rows affected when storing token for provider {} - provider may not exist!",
                provider_id
            );
        }

        let mut cache = self.cache.write().await;
        cache.insert(provider_id, token.to_string());

        tracing::debug!(provider_id = provider_id, "Token stored successfully");
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
            "SELECT encrypted_token, token_nonce FROM providers
             WHERE id = ?1 AND encrypted_token IS NOT NULL",
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to get token: {}", e)))?
        .ok_or_else(|| {
            DomainError::DatabaseError(format!("Token not found for provider {}", provider_id))
        })?;

        let token = self.decrypt_token(&row.1, &row.0).await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(provider_id, token.clone());
        }

        Ok(token)
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        sqlx::query(
            "UPDATE providers SET encrypted_token = NULL, token_nonce = NULL, updated_at = datetime('now')
             WHERE id = ?1",
        )
        .bind(provider_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to delete token: {}", e)))?;

        let mut cache = self.cache.write().await;
        cache.remove(&provider_id);

        tracing::debug!(provider_id = provider_id, "Token deleted successfully");
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
        tracing::info!(
            "[SqliteTokenStore] Starting import_encrypted (data size: {} bytes, password length: {})",
            data.len(),
            password.len()
        );

        if data.len() < 12 {
            return Err(DomainError::InvalidConfig("Invalid backup data".into()));
        }

        let nonce_bytes: [u8; 12] = data[..12]
            .try_into()
            .map_err(|_| DomainError::InvalidConfig("Invalid nonce".into()))?;
        let ciphertext = &data[12..];

        tracing::debug!(
            "[SqliteTokenStore] Extracted nonce (12 bytes) and ciphertext ({} bytes)",
            ciphertext.len()
        );

        let backup_key = Self::derive_backup_key(password);
        let cipher = Aes256Gcm::new_from_slice(&backup_key)
            .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {}", e)))?;

        let nonce = Nonce::from(nonce_bytes);

        tracing::debug!("[SqliteTokenStore] Attempting to decrypt token blob...");
        let plaintext = cipher.decrypt(&nonce, ciphertext).map_err(|_| {
            DomainError::AuthenticationFailed("Decryption failed (wrong password?)".into())
        })?;
        tracing::debug!(
            "[SqliteTokenStore] Decryption successful ({} bytes)",
            plaintext.len()
        );

        tracing::debug!("[SqliteTokenStore] Parsing JSON tokens...");
        let tokens: HashMap<i64, String> = serde_json::from_slice(&plaintext)
            .map_err(|e| DomainError::DatabaseError(format!("Failed to parse tokens: {}", e)))?;

        tracing::info!(
            "[SqliteTokenStore] Parsed {} tokens from encrypted blob",
            tokens.len()
        );

        let mut stored_count = 0;
        for (provider_id, token) in tokens {
            tracing::debug!(
                "[SqliteTokenStore] Importing token for provider {} (length: {})",
                provider_id,
                token.len()
            );
            self.store_token(provider_id, &token).await?;
            stored_count += 1;
        }

        tracing::info!(
            "[SqliteTokenStore] Import completed: {} tokens stored",
            stored_count
        );

        Ok(())
    }

    async fn warmup(&self) -> DomainResult<()> {
        tracing::info!("SQLite token store warmup complete (instant)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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

        sqlx::query(
            "INSERT INTO providers (name, provider_type) VALUES ('test-provider', 'github')",
        )
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
}
