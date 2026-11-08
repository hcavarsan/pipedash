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

type Tokens = HashMap<String, String>;

pub struct KeyringTokenStore {
    keyring_lock: Arc<Mutex<()>>,
    token_cache: Arc<Mutex<Option<Tokens>>>,
    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    entries: Arc<HashMap<String, Entry>>,
}

struct KeyringAccess {
    token_cache: Arc<Mutex<Option<Tokens>>>,
    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    entries: Arc<HashMap<String, Entry>>,
}

impl Default for KeyringTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

fn background_error(e: tokio::task::JoinError) -> DomainError {
    DomainError::InternalError(format!("Background task failed: {e}"))
}

impl KeyringAccess {
    fn with_entry<T>(
        &self, user: &str, operation: impl FnOnce(&Entry) -> crate::credential::Result<T>,
    ) -> crate::credential::Result<T> {
        #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
        if !self.entries.is_empty() {
            return operation(self.entries.get(user).ok_or(KeyringError::NoEntry)?);
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

    fn tokens(&self) -> DomainResult<Tokens> {
        let cached = self.token_cache.blocking_lock().clone();
        if let Some(cached_tokens) = cached {
            return Ok(cached_tokens);
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

        *self.token_cache.blocking_lock() = Some(tokens.clone());
        Ok(tokens)
    }

    fn save_tokens(&self, tokens: &Tokens) -> DomainResult<()> {
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

        *self.token_cache.blocking_lock() = Some(tokens.clone());

        Ok(())
    }

    fn migrate_legacy_token(
        &self, provider_id: i64, mut tokens: Tokens,
    ) -> DomainResult<Option<String>> {
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

        tokens.insert(provider_id.to_string(), token.clone());
        self.save_tokens(&tokens)?;

        self.delete_entry(&user)?;

        Ok(Some(token))
    }

    fn remove_token(&self, provider_id: i64) -> DomainResult<()> {
        self.delete_entry(&format!("provider_{provider_id}"))?;

        let mut tokens = self.tokens()?;
        tokens.remove(&provider_id.to_string());

        if tokens.is_empty() {
            self.delete_entry("tokens")?;
            *self.token_cache.blocking_lock() = Some(tokens);
            Ok(())
        } else {
            self.save_tokens(&tokens)
        }
    }
}

impl KeyringTokenStore {
    pub fn new() -> Self {
        Self {
            keyring_lock: Arc::new(Mutex::new(())),
            token_cache: Arc::new(Mutex::new(None)),
            #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
            entries: Arc::new(HashMap::new()),
        }
    }

    async fn run_serialized<T: Send + 'static>(
        &self, job: impl FnOnce(&KeyringAccess) -> DomainResult<T> + Send + 'static,
    ) -> DomainResult<T> {
        let _gate = self.keyring_lock.lock().await;
        let access = KeyringAccess {
            token_cache: self.token_cache.clone(),
            #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
            entries: self.entries.clone(),
        };
        tokio::task::spawn_blocking(move || job(&access))
            .await
            .map_err(background_error)?
    }
}

fn derive_backup_key(password: &str) -> DomainResult<[u8; 32]> {
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
        .map_err(|e| DomainError::InternalError(format!("Invalid Argon2 parameters: {e}")))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    argon2
        .hash_password_into(password.as_bytes(), BACKUP_KDF_SALT, &mut output)
        .map_err(|e| {
            DomainError::InternalError(format!("Failed to derive backup encryption key: {e}"))
        })?;

    Ok(output)
}

fn encrypt_backup(tokens: &HashMap<i64, String>, password: &str) -> DomainResult<Vec<u8>> {
    let json = serde_json::to_vec(tokens)
        .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize tokens: {}", e)))?;

    let backup_key = derive_backup_key(password)?;
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

fn decrypt_backup(data: &[u8], password: &str) -> DomainResult<HashMap<i64, String>> {
    if data.len() < 12 {
        return Err(DomainError::InvalidConfig("Invalid backup data".into()));
    }

    let nonce_bytes: [u8; 12] = data[..12]
        .try_into()
        .map_err(|_| DomainError::InvalidConfig("Invalid nonce".into()))?;
    let ciphertext = &data[12..];

    let backup_key = derive_backup_key(password)?;
    let cipher = Aes256Gcm::new_from_slice(&backup_key)
        .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {}", e)))?;

    let nonce = Nonce::from(nonce_bytes);

    let plaintext = cipher.decrypt(&nonce, ciphertext).map_err(|_| {
        DomainError::AuthenticationFailed("Decryption failed (wrong password?)".into())
    })?;

    serde_json::from_slice(&plaintext)
        .map_err(|e| DomainError::DatabaseError(format!("Failed to parse tokens: {}", e)))
}

#[async_trait]
impl TokenStore for KeyringTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let token = token.to_string();
        self.run_serialized(move |access| {
            let mut tokens = access.tokens()?;
            tokens.insert(provider_id.to_string(), token);
            access.save_tokens(&tokens)
        })
        .await
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        self.run_serialized(move |access| {
            let tokens = access.tokens()?;

            if let Some(token) = tokens.get(&provider_id.to_string()) {
                return Ok(token.clone());
            }

            access
                .migrate_legacy_token(provider_id, tokens)?
                .ok_or_else(|| {
                    DomainError::NotFound(format!(
                        "Token not found in keyring for provider {}",
                        provider_id
                    ))
                })
        })
        .await
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        self.run_serialized(move |access| access.remove_token(provider_id))
            .await
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        self.run_serialized(|access| {
            Ok(access
                .tokens()?
                .into_iter()
                .filter_map(|(key, value)| Some((key.parse::<i64>().ok()?, value)))
                .collect())
        })
        .await
    }

    async fn export_encrypted(&self, password: &str) -> DomainResult<Vec<u8>> {
        let tokens = self.get_all_tokens().await?;
        let password = password.to_string();

        tokio::task::spawn_blocking(move || encrypt_backup(&tokens, &password))
            .await
            .map_err(background_error)?
    }

    async fn import_encrypted(&self, data: &[u8], password: &str) -> DomainResult<()> {
        let data = data.to_vec();
        let password = password.to_string();

        let imported = tokio::task::spawn_blocking(move || decrypt_backup(&data, &password))
            .await
            .map_err(background_error)??;
        if imported.is_empty() {
            return Ok(());
        }

        self.run_serialized(move |access| {
            let mut tokens = access.tokens()?;
            tokens.extend(
                imported
                    .into_iter()
                    .map(|(provider_id, token)| (provider_id.to_string(), token)),
            );
            access.save_tokens(&tokens)
        })
        .await
    }
}

#[cfg(test)]
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod tests;
