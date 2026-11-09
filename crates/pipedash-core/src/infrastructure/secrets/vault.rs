use std::collections::HashMap;
use std::future::Future;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::secrets::crypto::{
    self,
    derive_key_blocking,
    KDF_ITERATIONS,
    KDF_MEMORY_KIB,
};

const VAULT_VERIFIER: &str = "pipedash-vault-verifier-v1";

pub(super) type TokenRow = (i64, Vec<u8>, Vec<u8>);

#[derive(sqlx::FromRow)]
pub(super) struct VerifierRow {
    pub(super) nonce: Vec<u8>,
    pub(super) ciphertext: Vec<u8>,
    pub(super) kdf_salt: Option<Vec<u8>>,
}

pub(super) fn vault_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseError(format!("Failed to access the token vault: {error}"))
}

pub(super) struct LegacyKdf {
    pub(super) salt: &'static [u8],
    pub(super) memory_kib: u32,
    pub(super) iterations: u32,
}

#[async_trait]
pub(super) trait VaultRows: Sync {
    async fn verifier(&self) -> DomainResult<Option<VerifierRow>>;
    async fn tokens(&self) -> DomainResult<Vec<TokenRow>>;
    async fn rekey(&self, from: &Vault, to: &Vault, verifier: VerifierRow) -> DomainResult<bool>;
}

pub(super) struct Vault {
    key: [u8; 32],
    legacy_key: Option<[u8; 32]>,
    cache: RwLock<HashMap<i64, String>>,
}

impl Vault {
    fn with_key(key: [u8; 32]) -> Self {
        Self {
            key,
            legacy_key: None,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub(super) async fn ephemeral(rows: &impl VaultRows) -> DomainResult<Self> {
        let vault = Self::with_key(rand::random());
        vault.verify(rows.verifier().await?)?;
        vault.load(rows.tokens().await?).await?;
        Ok(vault)
    }

    pub(super) async fn legacy(password: &str, legacy: &LegacyKdf) -> DomainResult<Self> {
        let key = derive_key_blocking(
            password,
            legacy.salt.to_vec(),
            legacy.memory_kib,
            legacy.iterations,
        )
        .await?;
        Ok(Self::with_key(key))
    }

    async fn salted(password: &str, salt: Vec<u8>) -> DomainResult<Self> {
        let key = derive_key_blocking(password, salt, KDF_MEMORY_KIB, KDF_ITERATIONS).await?;
        Ok(Self::with_key(key))
    }

    pub(super) async fn open(
        rows: &impl VaultRows, password: &str, legacy: &LegacyKdf,
    ) -> DomainResult<Self> {
        let verifier = rows.verifier().await?;
        if let Some(salt) = verifier.as_ref().and_then(|row| row.kdf_salt.clone()) {
            return Self::unlock(rows, password, salt, verifier, legacy).await;
        }

        let current = Self::legacy(password, legacy).await?;
        current.verify(verifier)?;
        current.load(rows.tokens().await?).await?;

        let salt = crypto::random_salt();
        let mut upgraded = Self::salted(password, salt.clone()).await?;
        let (nonce, ciphertext) = upgraded.seal_verifier()?;
        let sealed = VerifierRow {
            nonce,
            ciphertext,
            kdf_salt: Some(salt),
        };
        match rows.rekey(&current, &upgraded, sealed).await {
            Ok(true) => {
                upgraded.legacy_key = Some(current.key);
                upgraded.load(rows.tokens().await?).await?;
                tracing::info!("Vault key upgraded to a per-vault salt");
                Ok(upgraded)
            }
            Ok(false) => Box::pin(Self::open(rows, password, legacy)).await,
            Err(error) => {
                tracing::warn!(%error, "Vault key upgrade failed; keeping the current key");
                Ok(current)
            }
        }
    }

    async fn unlock(
        rows: &impl VaultRows, password: &str, salt: Vec<u8>, verifier: Option<VerifierRow>,
        legacy: &LegacyKdf,
    ) -> DomainResult<Self> {
        let mut vault = Self::salted(password, salt).await?;
        vault.verify(verifier)?;
        let tokens = rows.tokens().await?;
        if vault.load(tokens.clone()).await.is_ok() {
            return Ok(vault);
        }
        vault.legacy_key = Some(Self::legacy(password, legacy).await?.key);
        vault.load(tokens).await?;
        Ok(vault)
    }

    pub(super) fn encrypt(&self, plaintext: &str) -> DomainResult<(Vec<u8>, Vec<u8>)> {
        let (nonce, ciphertext) = crypto::encrypt(&self.key, plaintext.as_bytes())?;
        Ok((nonce.to_vec(), ciphertext))
    }

    pub(super) fn reseal(&self, from: &Vault, rows: Vec<TokenRow>) -> DomainResult<Vec<TokenRow>> {
        rows.into_iter()
            .map(|(provider_id, nonce, ciphertext)| {
                let (nonce, ciphertext) = self.encrypt(&from.decrypt(&nonce, &ciphertext)?)?;
                Ok((provider_id, nonce, ciphertext))
            })
            .collect()
    }

    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> DomainResult<String> {
        let plaintext = crypto::decrypt(&self.key, nonce, ciphertext).or_else(|error| {
            self.legacy_key
                .map_or(Err(error), |key| crypto::decrypt(&key, nonce, ciphertext))
        })?;
        String::from_utf8(plaintext)
            .map_err(|e| DomainError::DatabaseError(format!("Invalid UTF-8 in token: {}", e)))
    }

    fn seal_verifier(&self) -> DomainResult<(Vec<u8>, Vec<u8>)> {
        self.encrypt(VAULT_VERIFIER)
    }

    fn verify(&self, row: Option<VerifierRow>) -> DomainResult<()> {
        let Some(row) = row else {
            return Ok(());
        };

        crypto::decrypt(&self.key, &row.nonce, &row.ciphertext)
            .map_err(|_| DomainError::AuthenticationFailed("Wrong vault password".into()))?;
        Ok(())
    }

    pub(super) async fn load(&self, rows: Vec<TokenRow>) -> DomainResult<()> {
        let mut loaded = HashMap::new();
        let mut failed_count = 0;

        for (provider_id, nonce, ciphertext) in rows {
            match self.decrypt(&nonce, &ciphertext) {
                Ok(token) => {
                    loaded.insert(provider_id, token);
                }
                Err(e) => {
                    tracing::warn!(
                        provider_id = provider_id,
                        error = %e,
                        "Failed to decrypt token for provider"
                    );
                    failed_count += 1;
                }
            }
        }

        if failed_count > 0 {
            return Err(DomainError::AuthenticationFailed(format!(
                "Failed to decrypt {} token(s) - wrong vault password",
                failed_count
            )));
        }

        tracing::debug!(count = loaded.len(), "Loaded tokens to cache");
        *self.cache.write().await = loaded;
        Ok(())
    }

    pub(super) async fn get_or_load<F>(&self, provider_id: i64, fetch: F) -> DomainResult<String>
    where
        F: Future<Output = DomainResult<Option<(Vec<u8>, Vec<u8>)>>>,
    {
        if let Some(token) = self.cache.read().await.get(&provider_id) {
            return Ok(token.clone());
        }

        let (nonce, ciphertext) = fetch.await?.ok_or_else(|| {
            DomainError::NotFound(format!("Token not found for provider {}", provider_id))
        })?;
        let token = self.decrypt(&nonce, &ciphertext)?;
        self.remember(provider_id, token.clone()).await;
        Ok(token)
    }

    pub(super) async fn remember(&self, provider_id: i64, token: String) {
        self.cache.write().await.insert(provider_id, token);
    }

    pub(super) async fn forget(&self, provider_id: i64) {
        self.cache.write().await.remove(&provider_id);
    }

    pub(super) async fn all(&self) -> HashMap<i64, String> {
        self.cache.read().await.clone()
    }

    pub(super) async fn export(&self, password: &str) -> DomainResult<Vec<u8>> {
        let tokens = self.all().await;
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || crypto::seal_backup(&tokens, &password))
            .await
            .map_err(|e| DomainError::InternalError(format!("Backup task failed: {e}")))?
    }

    pub(super) async fn import(
        &self, data: &[u8], password: &str,
    ) -> DomainResult<HashMap<i64, String>> {
        let data = data.to_vec();
        let password = password.to_owned();
        let tokens = tokio::task::spawn_blocking(move || crypto::open_backup(&data, &password))
            .await
            .map_err(|e| DomainError::InternalError(format!("Backup task failed: {e}")))??;
        tracing::info!("Importing {} tokens from encrypted backup", tokens.len());
        Ok(tokens)
    }
}
