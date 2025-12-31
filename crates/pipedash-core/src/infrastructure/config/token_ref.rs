use std::fmt;

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

use crate::infrastructure::TokenStore;

#[derive(Debug, Error)]
pub enum TokenRefError {
    #[error("Plain-text token detected: {0}")]
    PlainTextToken(String),

    #[error("Invalid token reference format: {0}")]
    InvalidFormat(String),

    #[error("Failed to parse storage ID: {0}")]
    InvalidStorageId(#[from] std::num::ParseIntError),

    #[error("Environment variable not found: {0}")]
    EnvVarNotFound(String),

    #[error("Token not found in secure storage for provider: {0}")]
    NotFoundInStorage(i64),

    #[error("Token not configured")]
    NotConfigured,

    #[error("Keyring lookup failed: {0}")]
    KeyringError(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "value")]
pub enum TokenReference {
    EnvVar(String),
    SecureStorage(i64),
    Keyring(String),
    #[default]
    None,
}

impl fmt::Display for TokenReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvVar(name) => write!(f, "${{{}}} ", name),
            Self::SecureStorage(id) => write!(f, "storage:{}", id),
            Self::Keyring(name) => write!(f, "keyring:{}", name),
            Self::None => write!(f, "(not configured)"),
        }
    }
}

impl TokenReference {
    pub fn parse(value: &str) -> Result<Self, TokenRefError> {
        let value = value.trim();

        if value.is_empty() {
            return Ok(Self::None);
        }

        if value.starts_with("${") && value.ends_with('}') {
            let inner = &value[2..value.len() - 1];
            let var_name = inner.split(":-").next().unwrap_or(inner);
            if var_name.is_empty() {
                return Err(TokenRefError::InvalidFormat(
                    "Empty variable name in ${}".to_string(),
                ));
            }
            return Ok(Self::EnvVar(var_name.to_string()));
        }

        if let Some(var_name) = value.strip_prefix("env:") {
            if var_name.is_empty() {
                return Err(TokenRefError::InvalidFormat(
                    "Empty variable name after env:".to_string(),
                ));
            }
            return Ok(Self::EnvVar(var_name.to_string()));
        }

        if let Some(name) = value.strip_prefix("keyring:") {
            if name.is_empty() {
                return Err(TokenRefError::InvalidFormat(
                    "Empty name after keyring:".to_string(),
                ));
            }
            return Ok(Self::Keyring(name.to_string()));
        }

        if let Some(id_str) = value.strip_prefix("storage:") {
            let id: i64 = id_str.parse()?;
            return Ok(Self::SecureStorage(id));
        }

        if Self::looks_like_token(value) {
            return Err(TokenRefError::PlainTextToken(
                "Plain-text tokens are not allowed in config. Use ${ENV_VAR} or leave empty for keyring.".to_string(),
            ));
        }

        if Self::looks_like_env_var_name(value) {
            return Ok(Self::EnvVar(value.to_string()));
        }

        Err(TokenRefError::InvalidFormat(format!(
            "Unknown token reference format: '{}'. Use ${{ENV_VAR}}, env:VAR, keyring:name, or storage:id",
            value
        )))
    }

    pub fn to_toml_string(&self) -> String {
        match self {
            Self::EnvVar(name) => format!("${{{}}}", name),
            Self::SecureStorage(id) => format!("storage:{}", id),
            Self::Keyring(name) => format!("keyring:{}", name),
            Self::None => String::new(),
        }
    }

    pub fn is_plain_text(&self) -> bool {
        false
    }

    pub fn is_configured(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub async fn resolve(
        &self, token_store: &dyn TokenStore, provider_id: Option<i64>,
    ) -> Result<String, TokenRefError> {
        match self {
            Self::EnvVar(name) => {
                std::env::var(name).map_err(|_| TokenRefError::EnvVarNotFound(name.clone()))
            }
            Self::SecureStorage(id) => token_store
                .get_token(*id)
                .await
                .map_err(|_| TokenRefError::NotFoundInStorage(*id)),
            Self::Keyring(name) => token_store
                .get_token_by_name(name)
                .await
                .map_err(|e| TokenRefError::KeyringError(e.to_string())),
            Self::None => {
                if let Some(id) = provider_id {
                    token_store
                        .get_token(id)
                        .await
                        .map_err(|_| TokenRefError::NotFoundInStorage(id))
                } else {
                    Err(TokenRefError::NotConfigured)
                }
            }
        }
    }

    fn looks_like_token(value: &str) -> bool {
        if value.starts_with("ghp_")
            || value.starts_with("gho_")
            || value.starts_with("ghu_")
            || value.starts_with("ghs_")
            || value.starts_with("github_pat_")
        {
            return true;
        }

        if value.starts_with("glpat-") || value.starts_with("gldt-") {
            return true;
        }

        if value.starts_with("ATBB") {
            return true;
        }

        if value.len() >= 32
            && value
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_')
        {
            return true;
        }

        if value.len() > 30
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            let has_upper = value.chars().any(|c| c.is_ascii_uppercase());
            let has_lower = value.chars().any(|c| c.is_ascii_lowercase());
            let has_digit = value.chars().any(|c| c.is_ascii_digit());
            if (has_digit || has_upper) && has_lower || has_upper && has_digit {
                return true;
            }
        }

        false
    }

    fn looks_like_env_var_name(value: &str) -> bool {
        !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && value.chars().next().is_some_and(|c| c.is_ascii_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_var_dollar_brace() {
        let result = TokenReference::parse("${GITHUB_TOKEN}").unwrap();
        assert_eq!(result, TokenReference::EnvVar("GITHUB_TOKEN".to_string()));
    }

    #[test]
    fn test_parse_env_var_with_default() {
        let result = TokenReference::parse("${GITHUB_TOKEN:-}").unwrap();
        assert_eq!(result, TokenReference::EnvVar("GITHUB_TOKEN".to_string()));

        let result = TokenReference::parse("${GITHUB_TOKEN:-default_value}").unwrap();
        assert_eq!(result, TokenReference::EnvVar("GITHUB_TOKEN".to_string()));
    }

    #[test]
    fn test_parse_env_prefix() {
        let result = TokenReference::parse("env:MY_TOKEN").unwrap();
        assert_eq!(result, TokenReference::EnvVar("MY_TOKEN".to_string()));
    }

    #[test]
    fn test_parse_keyring() {
        let result = TokenReference::parse("keyring:my-provider").unwrap();
        assert_eq!(result, TokenReference::Keyring("my-provider".to_string()));
    }

    #[test]
    fn test_parse_storage() {
        let result = TokenReference::parse("storage:123").unwrap();
        assert_eq!(result, TokenReference::SecureStorage(123));
    }

    #[test]
    fn test_parse_empty() {
        let result = TokenReference::parse("").unwrap();
        assert_eq!(result, TokenReference::None);
    }

    #[test]
    fn test_parse_uppercase_as_env() {
        let result = TokenReference::parse("GITHUB_TOKEN").unwrap();
        assert_eq!(result, TokenReference::EnvVar("GITHUB_TOKEN".to_string()));
    }

    #[test]
    fn test_reject_github_token() {
        let result = TokenReference::parse("ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
        assert!(matches!(result, Err(TokenRefError::PlainTextToken(_))));
    }

    #[test]
    fn test_reject_gitlab_token() {
        let result = TokenReference::parse("glpat-xxxxxxxxxxxxxxxxxxxx");
        assert!(matches!(result, Err(TokenRefError::PlainTextToken(_))));
    }

    #[test]
    fn test_to_toml_string() {
        assert_eq!(
            TokenReference::EnvVar("TOKEN".to_string()).to_toml_string(),
            "${TOKEN}"
        );
        assert_eq!(
            TokenReference::SecureStorage(42).to_toml_string(),
            "storage:42"
        );
        assert_eq!(
            TokenReference::Keyring("my-key".to_string()).to_toml_string(),
            "keyring:my-key"
        );
        assert_eq!(TokenReference::None.to_toml_string(), "");
    }
}
