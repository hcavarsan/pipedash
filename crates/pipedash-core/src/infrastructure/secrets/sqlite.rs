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
use crate::infrastructure::secrets::vault::{
    vault_error,
    LegacyKdf,
    TokenRow,
    Vault,
    VaultRows,
    VerifierRow,
};
use crate::infrastructure::TokenStore;

const LEGACY_KDF: LegacyKdf = LegacyKdf {
    salt: b"pipedash-sqlite-vault-v1",
    memory_kib: 65536,
    iterations: 3,
};
const SELECT_TOKENS: &str = "SELECT id, token_nonce, encrypted_token FROM providers WHERE encrypted_token IS NOT NULL AND token_nonce IS NOT NULL";

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

        let vault = Vault::open(&SqliteRows(&pool), &password, &LEGACY_KDF).await?;
        tracing::info!("SQLite token store initialized");
        Ok(Self { pool, vault })
    }
}

struct SqliteRows<'a>(&'a Pool<Sqlite>);

#[async_trait]
impl VaultRows for SqliteRows<'_> {
    async fn verifier(&self) -> DomainResult<Option<VerifierRow>> {
        sqlx::query_as("SELECT nonce, ciphertext, kdf_salt FROM vault_verifier WHERE id = 1")
            .fetch_optional(self.0)
            .await
            .map_err(vault_error)
    }

    async fn tokens(&self) -> DomainResult<Vec<TokenRow>> {
        sqlx::query_as(SELECT_TOKENS)
            .fetch_all(self.0)
            .await
            .map_err(vault_error)
    }

    async fn rekey(&self, from: &Vault, to: &Vault, verifier: VerifierRow) -> DomainResult<bool> {
        let mut tx = self.0.begin().await.map_err(vault_error)?;
        let claimed = sqlx::query(include_str!(
            "../../../sql/secrets/upsert_verifier_sqlite.sql"
        ))
        .bind(&verifier.nonce)
        .bind(&verifier.ciphertext)
        .bind(&verifier.kdf_salt)
        .execute(&mut *tx)
        .await
        .map_err(vault_error)?
        .rows_affected();
        if claimed == 0 {
            return Ok(false);
        }

        let rows = sqlx::query_as(SELECT_TOKENS)
            .fetch_all(&mut *tx)
            .await
            .map_err(vault_error)?;
        for (provider_id, nonce, ciphertext) in to.reseal(from, rows)? {
            sqlx::query(
                "UPDATE providers SET encrypted_token = ?1, token_nonce = ?2 WHERE id = ?3",
            )
            .bind(&ciphertext)
            .bind(&nonce)
            .bind(provider_id)
            .execute(&mut *tx)
            .await
            .map_err(vault_error)?;
        }
        tx.commit().await.map_err(vault_error)?;
        Ok(true)
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
