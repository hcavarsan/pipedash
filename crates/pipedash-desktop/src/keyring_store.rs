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
use pipedash_core::domain::{
    DomainError,
    DomainResult,
};
use pipedash_core::infrastructure::TokenStore;
use tokio::sync::Mutex;

use crate::credential::{
    Entry,
    Error as KeyringError,
};

const BACKUP_KDF_SALT: &[u8] = b"pipedash-backup-salt-v1";

pub struct KeyringTokenStore {
    keyring_lock: Arc<Mutex<()>>,
    token_cache: Arc<Mutex<Option<HashMap<String, String>>>>,
    #[cfg(all(test, not(target_os = "ios")))]
    entries: HashMap<String, Entry>,
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
            #[cfg(all(test, not(target_os = "ios")))]
            entries: HashMap::new(),
        }
    }

    fn with_entry<T>(
        &self, user: &str, operation: impl FnOnce(&Entry) -> crate::credential::Result<T>,
    ) -> crate::credential::Result<T> {
        #[cfg(all(test, not(target_os = "ios")))]
        if !self.entries.is_empty() {
            return operation(
                self.entries
                    .get(user)
                    .expect("Missing isolated test credential"),
            );
        }
        operation(&Entry::new("pipedash", user)?)
    }

    fn delete_entry(&self, user: &str) -> DomainResult<()> {
        match self.with_entry(user, Entry::delete_credential) {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(DomainError::DatabaseError(format!(
                "Failed to delete keyring entry: {e}"
            ))),
        }
    }

    async fn get_all_tokens_cached(&self) -> DomainResult<HashMap<String, String>> {
        let mut cache = self.token_cache.lock().await;

        if let Some(ref cached_tokens) = *cache {
            return Ok(cached_tokens.clone());
        }

        let tokens = match self.with_entry("tokens", Entry::get_password) {
            Ok(json) => {
                if json.trim().is_empty() {
                    HashMap::new()
                } else {
                    serde_json::from_str(&json).map_err(|e| {
                        DomainError::DatabaseError(format!("Failed to parse tokens JSON: {e}"))
                    })?
                }
            }
            Err(KeyringError::NoEntry) => HashMap::new(),
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
        let json = serde_json::to_string(tokens)
            .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize tokens: {e}")))?;

        self.with_entry("tokens", |entry| entry.set_password(&json))
            .map_err(|e| {
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
        let user = format!("provider_{provider_id}");
        let token = match self.with_entry(&user, Entry::get_password) {
            Ok(token) => token,
            Err(KeyringError::NoEntry) => return Ok(None),
            Err(e) => {
                return Err(DomainError::DatabaseError(format!(
                    "Failed to read legacy keyring entry: {e}"
                )))
            }
        };

        let mut tokens = self.get_all_tokens_cached().await?;
        tokens.insert(provider_id.to_string(), token.clone());
        self.save_all_tokens(&tokens).await?;

        self.delete_entry(&user)?;

        Ok(Some(token))
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

        Err(DomainError::NotFound(format!(
            "Token not found in keyring for provider {}",
            provider_id
        )))
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        let _lock = self.keyring_lock.lock().await;
        self.delete_entry(&format!("provider_{provider_id}"))?;

        let mut tokens = self.get_all_tokens_cached().await?;
        tokens.remove(&provider_id.to_string());

        if tokens.is_empty() {
            self.delete_entry("tokens")?;
            *self.token_cache.lock().await = Some(tokens);
            Ok(())
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

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
    use keyring_core::mock::Cred;

    use super::*;

    fn store() -> KeyringTokenStore {
        KeyringTokenStore {
            entries: ["tokens", "provider_1", "provider_2"]
                .into_iter()
                .map(|user| {
                    (
                        user.to_string(),
                        Entry {
                            inner: keyring_core::Entry::new_with_credential(Arc::new(Cred {
                                specifiers: ("pipedash".into(), user.into()),
                                inner: Default::default(),
                            })),
                        },
                    )
                })
                .collect(),
            ..KeyringTokenStore::new()
        }
    }

    fn fail_next(store: &KeyringTokenStore, user: &str) {
        store.entries[user]
            .inner
            .as_any()
            .downcast_ref::<Cred>()
            .unwrap()
            .set_error(KeyringError::Invalid(
                "locked".into(),
                "access denied".into(),
            ));
    }

    #[tokio::test]
    async fn deleting_the_last_token_does_not_resurrect_it_when_adding_another() {
        let store = store();
        store.store_token(1, "old-token").await.unwrap();
        store.entries["provider_1"]
            .set_password("legacy-token")
            .unwrap();

        store.delete_token(1).await.unwrap();

        assert!(store.get_all_tokens().await.unwrap().is_empty());
        assert!(matches!(
            store.entries["tokens"].get_password(),
            Err(KeyringError::NoEntry)
        ));
        assert!(matches!(
            store.get_token(1).await,
            Err(DomainError::NotFound(_))
        ));
        store.store_token(2, "new-token").await.unwrap();
        *store.token_cache.lock().await = None;
        assert_eq!(
            store.get_all_tokens().await.unwrap(),
            HashMap::from([(2, "new-token".into())])
        );
    }

    #[tokio::test]
    async fn deleting_an_absent_token_is_idempotent() {
        let store = store();
        store.delete_token(1).await.unwrap();
        store.delete_token(1).await.unwrap();
        assert!(store.get_all_tokens().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_persistent_deletion_keeps_the_cache_and_supports_retry() {
        let store = store();
        store.store_token(1, "old-token").await.unwrap();
        fail_next(&store, "tokens");

        assert!(store.delete_token(1).await.is_err());
        assert_eq!(store.get_token(1).await.unwrap(), "old-token");
        assert!(store.entries["tokens"].get_password().is_ok());

        store.delete_token(1).await.unwrap();
        assert!(store.get_all_tokens().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_legacy_deletion_does_not_remove_the_current_token() {
        let store = store();
        store.store_token(1, "current-token").await.unwrap();
        store.entries["provider_1"]
            .set_password("legacy-token")
            .unwrap();
        fail_next(&store, "provider_1");

        assert!(store.delete_token(1).await.is_err());
        assert_eq!(store.get_token(1).await.unwrap(), "current-token");
        assert_eq!(
            store.entries["provider_1"].get_password().unwrap(),
            "legacy-token"
        );

        store.delete_token(1).await.unwrap();
        assert!(matches!(
            store.get_token(1).await,
            Err(DomainError::NotFound(_))
        ));
    }
}
