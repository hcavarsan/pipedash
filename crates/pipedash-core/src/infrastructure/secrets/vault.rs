use std::collections::HashMap;
use std::future::Future;

use tokio::sync::RwLock;

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::secrets::crypto::{
    self,
    derive_key_blocking,
};

const BACKUP_KDF_SALT: &[u8] = b"pipedash-backup-salt-v1";
const VAULT_VERIFIER: &str = "pipedash-vault-verifier-v1";

pub(super) struct Vault {
    key: [u8; 32],
    kdf_memory_kib: u32,
    kdf_iterations: u32,
    cache: RwLock<HashMap<i64, String>>,
}

impl Vault {
    pub(super) async fn derive(
        password: &str, salt: &'static [u8], kdf_memory_kib: u32, kdf_iterations: u32,
    ) -> DomainResult<Self> {
        Ok(Self {
            key: derive_key_blocking(password, salt, kdf_memory_kib, kdf_iterations).await?,
            kdf_memory_kib,
            kdf_iterations,
            cache: RwLock::new(HashMap::new()),
        })
    }

    pub(super) fn encrypt(&self, plaintext: &str) -> DomainResult<(Vec<u8>, Vec<u8>)> {
        let (nonce, ciphertext) = crypto::encrypt(&self.key, plaintext.as_bytes())?;
        Ok((nonce.to_vec(), ciphertext))
    }

    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> DomainResult<String> {
        let plaintext = crypto::decrypt(&self.key, nonce, ciphertext)?;
        String::from_utf8(plaintext)
            .map_err(|e| DomainError::DatabaseError(format!("Invalid UTF-8 in token: {}", e)))
    }

    pub(super) fn seal_verifier(&self) -> DomainResult<(Vec<u8>, Vec<u8>)> {
        self.encrypt(VAULT_VERIFIER)
    }

    pub(super) fn verify(&self, row: Option<(Vec<u8>, Vec<u8>)>) -> DomainResult<bool> {
        let Some((nonce, ciphertext)) = row else {
            return Ok(false);
        };

        self.decrypt(&nonce, &ciphertext)
            .map_err(|_| DomainError::AuthenticationFailed("Wrong vault password".into()))?;
        Ok(true)
    }

    pub(super) async fn load(&self, rows: Vec<(i64, Vec<u8>, Vec<u8>)>) -> DomainResult<()> {
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

    async fn backup_key(&self, password: &str) -> DomainResult<[u8; 32]> {
        derive_key_blocking(
            password,
            BACKUP_KDF_SALT,
            self.kdf_memory_kib,
            self.kdf_iterations,
        )
        .await
    }

    pub(super) async fn export(&self, password: &str) -> DomainResult<Vec<u8>> {
        let backup_key = self.backup_key(password).await?;
        let tokens = self.all().await;
        crypto::encode_backup(&tokens, &backup_key)
    }

    pub(super) async fn import(
        &self, data: &[u8], password: &str,
    ) -> DomainResult<HashMap<i64, String>> {
        let backup_key = self.backup_key(password).await?;
        let tokens = crypto::decode_backup(data, &backup_key)?;
        tracing::info!("Importing {} tokens from encrypted backup", tokens.len());
        Ok(tokens)
    }
}
