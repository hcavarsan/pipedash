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
use keyring::Entry;
use pipedash_core::domain::{
    DomainError,
    DomainResult,
};
use pipedash_core::infrastructure::TokenStore;
use tokio::sync::Mutex;

const BACKUP_KDF_SALT: &[u8] = b"pipedash-backup-salt-v1";

pub struct KeyringTokenStore {
    keyring_lock: Arc<Mutex<()>>,
    token_cache: Arc<Mutex<Option<HashMap<String, String>>>>,
}

impl Default for KeyringTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringTokenStore {
    pub fn new() -> Self {
        Self {
            keyring_lock: Arc::new(Mutex::new(())),
            token_cache: Arc::new(Mutex::new(None)),
        }
    }

    fn keyring_entry(&self) -> DomainResult<Entry> {
        Entry::new("pipedash", "tokens")
            .map_err(|e| DomainError::DatabaseError(format!("Failed to create keyring entry: {e}")))
    }

    async fn get_all_tokens_cached(&self) -> DomainResult<HashMap<String, String>> {
        let mut cache = self.token_cache.lock().await;

        if let Some(ref cached_tokens) = *cache {
            return Ok(cached_tokens.clone());
        }

        let entry = self.keyring_entry()?;
        let tokens = match entry.get_password() {
            Ok(json) => {
                if json.trim().is_empty() {
                    HashMap::new()
                } else {
                    serde_json::from_str(&json).map_err(|e| {
                        DomainError::DatabaseError(format!("Failed to parse tokens JSON: {e}"))
                    })?
                }
            }
            Err(keyring::Error::NoEntry) => HashMap::new(),
            Err(e) => {
                return Err(DomainError::DatabaseError(format!(
                    "Failed to get tokens from keyring: {e}"
                )))
            }
        };

        *cache = Some(tokens.clone());
        Ok(tokens)
    }

    async fn save_all_tokens(&self, tokens: &HashMap<String, String>) -> DomainResult<()> {
        let entry = self.keyring_entry()?;
        let json = serde_json::to_string(tokens)
            .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize tokens: {e}")))?;

        entry.set_password(&json).map_err(|e| {
            DomainError::DatabaseError(format!(
                "Failed to store tokens in system keyring: {}\n\
                 \nThe tokens will not be saved securely. Please ensure:\n\
                 - macOS: Grant Keychain Access permission to Pipedash\n\
                 - Linux: Install libsecret (sudo apt install libsecret-1-dev)\n\
                 - Windows: Ensure Credential Manager is accessible",
                e
            ))
        })?;

        let mut cache = self.token_cache.lock().await;
        *cache = Some(tokens.clone());

        Ok(())
    }

    async fn migrate_legacy_token(&self, provider_id: i64) -> DomainResult<Option<String>> {
        let old_entry =
            Entry::new("pipedash", &format!("provider_{}", provider_id)).map_err(|e| {
                DomainError::DatabaseError(format!("Failed to create old keyring entry: {e}"))
            })?;

        if let Ok(token) = old_entry.get_password() {
            let mut tokens = self.get_all_tokens_cached().await?;
            tokens.insert(provider_id.to_string(), token.clone());
            self.save_all_tokens(&tokens).await?;

            let _ = old_entry.delete_credential();

            return Ok(Some(token));
        }

        Ok(None)
    }

    pub async fn cleanup_legacy_entries(&self) -> DomainResult<usize> {
        let _lock = self.keyring_lock.lock().await;
        let mut cleaned = 0;

        for provider_id in 1..=1000 {
            let old_entry = match Entry::new("pipedash", &format!("provider_{}", provider_id)) {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            if old_entry.delete_credential().is_ok() {
                cleaned += 1;
                tracing::debug!(
                    "[KeyringTokenStore] Cleaned up legacy entry for provider_{}",
                    provider_id
                );
            }
        }

        if cleaned > 0 {
            tracing::info!(
                "[KeyringTokenStore] Cleaned up {} legacy keyring entries",
                cleaned
            );
        }

        Ok(cleaned)
    }

    fn derive_backup_key(password: &str) -> [u8; 32] {
        use argon2::{
            Argon2,
            ParamsBuilder,
        };

        let mut output = [0u8; 32];

        let params = ParamsBuilder::new()
            .m_cost(4096) // 4 MiB memory
            .t_cost(1) // 1 iteration
            .p_cost(1) // 1 thread
            .output_len(32) // 256-bit key
            .build()
            .expect("Invalid Argon2 parameters");

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        argon2
            .hash_password_into(password.as_bytes(), BACKUP_KDF_SALT, &mut output)
            .expect("Failed to derive backup encryption key");

        output
    }
}

#[async_trait]
impl TokenStore for KeyringTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let _lock = self.keyring_lock.lock().await;

        let mut tokens = self.get_all_tokens_cached().await?;
        tokens.insert(provider_id.to_string(), token.to_string());
        self.save_all_tokens(&tokens).await
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        let _lock = self.keyring_lock.lock().await;

        let tokens = self.get_all_tokens_cached().await?;

        if let Some(token) = tokens.get(&provider_id.to_string()) {
            return Ok(token.clone());
        }

        drop(tokens);
        if let Some(token) = self.migrate_legacy_token(provider_id).await? {
            return Ok(token);
        }

        Err(DomainError::DatabaseError(format!(
            "Token not found in keyring for provider {}",
            provider_id
        )))
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        let _lock = self.keyring_lock.lock().await;

        let mut tokens = self.get_all_tokens_cached().await?;
        tokens.remove(&provider_id.to_string());

        if tokens.is_empty() {
            let entry = self.keyring_entry()?;
            entry.delete_credential().map_err(|e| {
                DomainError::DatabaseError(format!("Failed to delete keyring entry: {e}"))
            })
        } else {
            self.save_all_tokens(&tokens).await
        }
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        let _lock = self.keyring_lock.lock().await;

        let string_tokens = self.get_all_tokens_cached().await?;
        let mut result = HashMap::new();

        for (key, value) in string_tokens {
            if let Ok(id) = key.parse::<i64>() {
                result.insert(id, value);
            }
        }

        Ok(result)
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

pub fn create_keyring_token_store() -> Arc<dyn TokenStore> {
    Arc::new(KeyringTokenStore::new())
}
