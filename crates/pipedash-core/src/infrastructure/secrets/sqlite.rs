use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::{
    Pool,
    Sqlite,
};

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::secrets::vault::Vault;
use crate::infrastructure::TokenStore;

const SQLITE_KDF_SALT: &[u8] = b"pipedash-sqlite-vault-v1";
const KDF_MEMORY_KIB: u32 = 65536;
const KDF_ITERATIONS: u32 = 3;

pub struct SqliteTokenStore {
    pool: Pool<Sqlite>,
    vault: Vault,
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

        let vault =
            Vault::derive(&password, SQLITE_KDF_SALT, KDF_MEMORY_KIB, KDF_ITERATIONS).await?;
        let store = Self { pool, vault };

        let has_verifier = store.check_verifier().await?;
        store.load_to_cache().await?;
        if !has_verifier {
            store.store_verifier().await?;
        }

        tracing::info!("SQLite token store initialized");
        Ok(store)
    }

    async fn check_verifier(&self) -> DomainResult<bool> {
        let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
            "SELECT nonce, ciphertext FROM vault_verifier WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to read vault verifier: {}", e)))?;

        self.vault.verify(row)
    }

    async fn store_verifier(&self) -> DomainResult<()> {
        let (nonce, ciphertext) = self.vault.seal_verifier()?;

        sqlx::query(
            "INSERT INTO vault_verifier (id, nonce, ciphertext) VALUES (1, ?1, ?2)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&nonce)
        .bind(&ciphertext)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DomainError::DatabaseError(format!("Failed to store vault verifier: {}", e))
        })?;

        self.check_verifier().await?;
        Ok(())
    }

    async fn load_to_cache(&self) -> DomainResult<()> {
        let rows = sqlx::query_as::<_, (i64, Vec<u8>, Vec<u8>)>(
            "SELECT id, token_nonce, encrypted_token FROM providers
             WHERE encrypted_token IS NOT NULL AND token_nonce IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to load tokens: {}", e)))?;

        self.vault.load(rows).await
    }
}

#[async_trait]
impl TokenStore for SqliteTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let (nonce, ciphertext) = self.vault.encrypt(token)?;

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

        if result.rows_affected() == 0 {
            tracing::warn!(
                "[SqliteTokenStore] No rows affected when storing token for provider {} - provider may not exist!",
                provider_id
            );
        }

        self.vault.remember(provider_id, token.to_string()).await;
        tracing::debug!(provider_id = provider_id, "Token stored successfully");
        Ok(())
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        self.vault
            .get_or_load(provider_id, async {
                sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
                    "SELECT token_nonce, encrypted_token FROM providers
                     WHERE id = ?1 AND encrypted_token IS NOT NULL",
                )
                .bind(provider_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(format!("Failed to get token: {}", e)))
            })
            .await
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

        self.vault.forget(provider_id).await;
        tracing::debug!(provider_id = provider_id, "Token deleted successfully");
        Ok(())
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        Ok(self.vault.all().await)
    }

    async fn export_encrypted(&self, password: &str) -> DomainResult<Vec<u8>> {
        self.vault.export(password).await
    }

    async fn import_encrypted(&self, data: &[u8], password: &str) -> DomainResult<()> {
        for (provider_id, token) in self.vault.import(data, password).await? {
            self.store_token(provider_id, &token).await?;
        }
        Ok(())
    }

    async fn warmup(&self) -> DomainResult<()> {
        tracing::info!("SQLite token store warmup complete (instant)");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
