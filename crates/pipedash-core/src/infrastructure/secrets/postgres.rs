use std::collections::HashMap;

use async_trait::async_trait;
use rand::RngExt;
use sqlx::{
    Pool,
    Postgres,
};

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::secrets::vault::Vault;
use crate::infrastructure::storage::postgres::{
    connect,
    pool_options,
};
use crate::infrastructure::TokenStore;

const ENCRYPTION_KDF_SALT: &[u8] = b"pipedash-postgres-salt-v1";
const KDF_MEMORY_KIB: u32 = 4096;
const KDF_ITERATIONS: u32 = 1;

pub struct PostgresTokenStore {
    pool: Pool<Postgres>,
    vault: Vault,
}

impl PostgresTokenStore {
    pub async fn new(
        connection_string: &str, vault_password: Option<String>,
    ) -> DomainResult<Self> {
        let options = pool_options().after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO public")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        });
        let pool = connect(options, connection_string).await?;

        let password = vault_password.or_else(|| std::env::var("PIPEDASH_VAULT_PASSWORD").ok());
        let generated_key = password.is_none();
        let password = password.unwrap_or_else(Self::generate_random_key);

        let vault = Vault::derive(
            &password,
            ENCRYPTION_KDF_SALT,
            KDF_MEMORY_KIB,
            KDF_ITERATIONS,
        )
        .await?;
        let store = Self { pool, vault };

        let has_verifier = store.check_verifier().await?;
        store.load_to_cache().await?;
        if !has_verifier && !generated_key {
            store.store_verifier().await?;
        }

        tracing::info!("PostgreSQL token store initialized");

        Ok(store)
    }

    fn generate_random_key() -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::rng();
        (0..64)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
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
            "INSERT INTO vault_verifier (id, nonce, ciphertext) VALUES (1, $1, $2)
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
            "SELECT provider_id, nonce, ciphertext FROM encrypted_tokens",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to load tokens: {}", e)))?;

        self.vault.load(rows).await
    }
}

#[async_trait]
impl TokenStore for PostgresTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let (nonce, ciphertext) = self.vault.encrypt(token)?;

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

        self.vault.remember(provider_id, token.to_string()).await;
        Ok(())
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        self.vault
            .get_or_load(provider_id, async {
                sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
                    "SELECT nonce, ciphertext FROM encrypted_tokens WHERE provider_id = $1",
                )
                .bind(provider_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(format!("Failed to get token: {}", e)))
            })
            .await
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        sqlx::query("DELETE FROM encrypted_tokens WHERE provider_id = $1")
            .bind(provider_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to delete token: {}", e)))?;

        self.vault.forget(provider_id).await;
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
}

#[cfg(test)]
mod tests;
