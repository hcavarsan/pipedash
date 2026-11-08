use std::collections::BTreeMap;
use std::fmt;

use serde::{
    Deserialize,
    Serialize,
};
use url::Url;

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::{
    MigrationStats,
    PipedashConfig,
    StorageBackendType,
};

pub const POSTGRES_URL_ERROR: &str = "Enter a valid PostgreSQL connection URL";
pub const PASSWORD_LENGTH_ERROR: &str = "Use at least 8 characters";
pub const PASSWORD_MISMATCH_ERROR: &str = "Passwords do not match";

const MIN_PASSWORD_LEN: usize = 8;
const STRENGTH_CHECK_MIN_LEN: usize = 12;
const STRONG_PASSWORD_LEN: usize = 16;
const PASSWORD_SPECIALS: &str = "!@#$%^&*()_+-=[]{};':\"\\|,.<>/?";

#[derive(Clone, Serialize, Deserialize)]
pub struct SetupDraft {
    pub backend: String,
    pub data_dir: String,
    pub postgres_url: String,
    pub vault_password: String,
    pub vault_password_confirm: String,
    pub transfer_data: bool,
}

impl fmt::Debug for SetupDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetupDraft")
            .field("backend", &self.backend)
            .field("data_dir", &self.data_dir)
            .field("transfer_data", &self.transfer_data)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupDraftResult {
    pub config: PipedashConfig,
    pub errors: BTreeMap<String, String>,
    pub password_warnings: Vec<String>,
    pub needs_migration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupApplied {
    pub saved: bool,
    pub migration_stats: Option<MigrationStats>,
    pub restart_required: bool,
}

impl SetupDraft {
    pub fn initial(current: Option<&PipedashConfig>, default_data_dir: &str) -> Self {
        let storage = current.map(|config| &config.storage);
        let data_dir = storage
            .map(|s| s.data_dir.as_str())
            .filter(|dir| !dir.is_empty())
            .unwrap_or(default_data_dir);

        Self {
            backend: storage.map_or_else(|| "sqlite".into(), |s| s.backend.to_string()),
            data_dir: data_dir.into(),
            postgres_url: storage
                .map(|s| s.postgres.connection_string.clone())
                .unwrap_or_default(),
            vault_password: String::new(),
            vault_password_confirm: String::new(),
            transfer_data: true,
        }
    }

    pub fn storage_backend(&self) -> DomainResult<StorageBackendType> {
        self.backend.parse().map_err(DomainError::InvalidConfig)
    }

    pub fn validate(&self, needs_vault_password: bool) -> BTreeMap<String, String> {
        let mut errors = BTreeMap::new();

        if self.storage_backend().is_ok_and(|b| b.requires_postgres())
            && !is_postgres_url(&self.postgres_url)
        {
            errors.insert("postgres_url".into(), POSTGRES_URL_ERROR.into());
        }

        if needs_vault_password {
            if self.vault_password.chars().count() < MIN_PASSWORD_LEN {
                errors.insert("vault_password".into(), PASSWORD_LENGTH_ERROR.into());
            }
            if self.vault_password != self.vault_password_confirm {
                errors.insert(
                    "vault_password_confirm".into(),
                    PASSWORD_MISMATCH_ERROR.into(),
                );
            }
        }

        errors
    }

    pub fn password_warnings(&self) -> Vec<String> {
        if self.vault_password.chars().count() < STRENGTH_CHECK_MIN_LEN {
            return Vec::new();
        }

        analyze_password_strength(&self.vault_password)
    }
}

pub fn is_postgres_url(value: &str) -> bool {
    Url::parse(value.trim()).is_ok_and(|url| {
        matches!(url.scheme(), "postgres" | "postgresql")
            && url.host_str().is_some_and(|host| !host.is_empty())
    })
}

fn analyze_password_strength(password: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    if password.chars().count() < STRONG_PASSWORD_LEN {
        warnings.push("Consider using 16+ characters");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        warnings.push("Consider adding numbers");
    }
    if !password.chars().any(|c| PASSWORD_SPECIALS.contains(c)) {
        warnings.push("Consider adding special characters");
    }
    if !password.chars().any(|c| c.is_ascii_uppercase())
        || !password.chars().any(|c| c.is_ascii_lowercase())
    {
        warnings.push("Consider mixing uppercase and lowercase");
    }

    warnings.into_iter().map(String::from).collect()
}
