use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::{
    Pool,
    Postgres,
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
use crate::infrastructure::storage::postgres::{
    connect,
    pool_options,
};
use crate::infrastructure::TokenStore;

const LEGACY_KDF: LegacyKdf = LegacyKdf {
    salt: b"pipedash-postgres-salt-v1",
    memory_kib: 4096,
    iterations: 1,
};
const SELECT_TOKENS: &str = "SELECT provider_id, nonce, ciphertext FROM encrypted_tokens";

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

        let rows = PostgresRows(&pool);
        let vault = match vault_password.or_else(|| std::env::var("PIPEDASH_VAULT_PASSWORD").ok()) {
            Some(password) => Vault::open(&rows, &password, &LEGACY_KDF).await?,
            None => Vault::ephemeral(&rows).await?,
        };

        tracing::info!("PostgreSQL token store initialized");

        Ok(Self { pool, vault })
    }
}

struct PostgresRows<'a>(&'a Pool<Postgres>);

#[async_trait]
impl VaultRows for PostgresRows<'_> {
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
        sqlx::query("LOCK TABLE encrypted_tokens IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await
            .map_err(vault_error)?;
        let claimed = sqlx::query(include_str!(
            "../../../sql/secrets/upsert_verifier_postgres.sql"
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
                "UPDATE encrypted_tokens SET nonce = $1, ciphertext = $2 WHERE provider_id = $3",
            )
            .bind(&nonce)
            .bind(&ciphertext)
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
