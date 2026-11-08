use serde::Serialize;

use crate::infrastructure::database::has_encrypted_tokens;
use crate::infrastructure::migration::VAULT_PASSWORD_ENV_VAR;
use crate::infrastructure::PipedashConfig;

#[derive(Debug, Serialize)]
pub struct VaultPasswordStatus {
    pub is_set: bool,
    pub env_var_name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PasswordSource {
    EnvVar,
    Keyring,
    Session,
    None,
}

#[derive(Debug, Serialize)]
pub struct VaultStatus {
    pub is_unlocked: bool,
    pub password_source: PasswordSource,
    pub backend: String,
    pub requires_password: bool,
    pub is_first_time: bool,
}

#[derive(Debug, Serialize)]
pub struct UnlockVaultResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl UnlockVaultResponse {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            reason: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            reason: None,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            success: false,
            message: "cancelled".to_string(),
            reason: Some("cancelled".to_string()),
        }
    }
}

pub fn vault_password_status() -> VaultPasswordStatus {
    VaultPasswordStatus {
        is_set: std::env::var(VAULT_PASSWORD_ENV_VAR).is_ok(),
        env_var_name: VAULT_PASSWORD_ENV_VAR,
    }
}

pub async fn requires_vault_password(config: &PipedashConfig) -> bool {
    if config.storage.backend.is_sqlite() {
        has_encrypted_tokens(&config.data_dir().join("pipedash.db")).await
    } else {
        true
    }
}

pub fn vault_is_unlocked(requires_password: bool, password_set: bool, app_locked: bool) -> bool {
    (!requires_password || password_set) && !app_locked
}

#[cfg(test)]
mod tests;
