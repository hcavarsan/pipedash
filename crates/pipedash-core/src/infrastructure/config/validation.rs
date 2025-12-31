use indexmap::IndexMap;

use super::schema::{
    PipedashConfig,
    ProviderFileConfig,
    StorageBackend,
};

#[derive(Debug, Default)]
pub struct ValidationResult {
    pub errors: Vec<ConfigError>,
    pub warnings: Vec<ConfigWarning>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_issues(&self) -> bool {
        !self.errors.is_empty() || !self.warnings.is_empty()
    }

    pub fn add_error(&mut self, error: ConfigError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: ConfigWarning) {
        self.warnings.push(warning);
    }

    pub fn summary(&self) -> String {
        if self.errors.is_empty() && self.warnings.is_empty() {
            "Configuration is valid".to_string()
        } else {
            format!(
                "{} error(s), {} warning(s)",
                self.errors.len(),
                self.warnings.len()
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigError {
    pub field: String,
    pub message: String,
    pub code: ConfigErrorCode,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.field, self.message, self.code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigErrorCode {
    MissingRequired,
    InvalidValue,
    PlainTextToken,
    BackendMismatch,
    FeatureNotEnabled,
}

impl std::fmt::Display for ConfigErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequired => write!(f, "MISSING_REQUIRED"),
            Self::InvalidValue => write!(f, "INVALID_VALUE"),
            Self::PlainTextToken => write!(f, "PLAIN_TEXT_TOKEN"),
            Self::BackendMismatch => write!(f, "BACKEND_MISMATCH"),
            Self::FeatureNotEnabled => write!(f, "FEATURE_NOT_ENABLED"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigWarning {
    pub field: String,
    pub message: String,
    pub code: ConfigWarningCode,
}

impl std::fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.field, self.message, self.code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWarningCode {
    NoTokenConfigured,
    InsecureDefault,
    Deprecated,
    UnusedSetting,
}

impl std::fmt::Display for ConfigWarningCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTokenConfigured => write!(f, "NO_TOKEN"),
            Self::InsecureDefault => write!(f, "INSECURE_DEFAULT"),
            Self::Deprecated => write!(f, "DEPRECATED"),
            Self::UnusedSetting => write!(f, "UNUSED"),
        }
    }
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate(config: &PipedashConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        Self::validate_storage(&config.storage, &mut result);
        Self::validate_providers(&config.providers, &mut result);

        result
    }

    fn validate_storage(storage: &super::schema::StorageConfig, result: &mut ValidationResult) {
        if storage.backend == StorageBackend::Postgres
            && storage.postgres.connection_string.is_empty()
        {
            result.add_error(ConfigError {
                field: "storage.postgres.connection_string".to_string(),
                message: "PostgreSQL connection string is required when backend = 'postgres'"
                    .to_string(),
                code: ConfigErrorCode::MissingRequired,
            });
        }

        #[cfg(not(feature = "postgres"))]
        if storage.backend == StorageBackend::Postgres {
            result.add_error(ConfigError {
                field: "storage.backend".to_string(),
                message: "PostgreSQL feature is not enabled. Compile with --features postgres"
                    .to_string(),
                code: ConfigErrorCode::FeatureNotEnabled,
            });
        }
    }

    fn validate_providers(
        providers: &IndexMap<String, ProviderFileConfig>, result: &mut ValidationResult,
    ) {
        for (id, provider) in providers {
            let prefix = format!("providers.{}", id);

            if id.is_empty() {
                result.add_error(ConfigError {
                    field: "providers".to_string(),
                    message: "Provider ID (table key) cannot be empty".to_string(),
                    code: ConfigErrorCode::MissingRequired,
                });
            }

            if provider.provider_type.is_empty() {
                result.add_error(ConfigError {
                    field: format!("{}.type", prefix),
                    message: "Provider type is required".to_string(),
                    code: ConfigErrorCode::MissingRequired,
                });
            } else {
                let valid_types = [
                    "github",
                    "gitlab",
                    "bitbucket",
                    "buildkite",
                    "jenkins",
                    "tekton",
                    "argocd",
                ];
                if !valid_types.contains(&provider.provider_type.as_str()) {
                    result.add_warning(ConfigWarning {
                        field: format!("{}.type", prefix),
                        message: format!(
                            "Unknown provider type: '{}'. Valid types: {:?}",
                            provider.provider_type, valid_types
                        ),
                        code: ConfigWarningCode::UnusedSetting,
                    });
                }
            }

            if provider.token.is_empty() {
                result.add_warning(ConfigWarning {
                    field: format!("{}.token", prefix),
                    message: format!(
                        "Provider '{}' has no token configured. Will use secure storage lookup.",
                        id
                    ),
                    code: ConfigWarningCode::NoTokenConfigured,
                });
            }

            if provider.refresh_interval == 0 {
                result.add_warning(ConfigWarning {
                    field: format!("{}.refresh_interval", prefix),
                    message: "Refresh interval of 0 means manual refresh only".to_string(),
                    code: ConfigWarningCode::UnusedSetting,
                });
            }
        }
    }
}

impl PipedashConfig {
    pub fn validate(&self) -> ValidationResult {
        ConfigValidator::validate(self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_valid_config() {
        let config = PipedashConfig::default();
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_postgres_missing_connection_string() {
        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackend::Postgres;

        let result = config.validate();
        assert!(!result.is_ok());
        assert!(result
            .errors
            .iter()
            .any(|e| e.field.contains("postgres.connection_string")));
    }

    #[test]
    fn test_provider_missing_type() {
        let mut config = PipedashConfig::default();
        config.providers.insert(
            "test".to_string(),
            ProviderFileConfig {
                name: None,
                provider_type: String::new(),
                token: String::new(),
                refresh_interval: 30,
                config: HashMap::new(),
            },
        );

        let result = config.validate();
        assert!(result
            .errors
            .iter()
            .any(|e| e.field.contains(".type") && e.code == ConfigErrorCode::MissingRequired));
    }

    #[test]
    fn test_provider_any_token_format_allowed() {
        let mut config = PipedashConfig::default();
        config.providers.insert(
            "test".to_string(),
            ProviderFileConfig {
                name: None,
                provider_type: "github".to_string(),
                token: "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                refresh_interval: 30,
                config: HashMap::new(),
            },
        );

        let result = config.validate();
        assert!(!result
            .errors
            .iter()
            .any(|e| e.code == ConfigErrorCode::PlainTextToken));
    }

    #[test]
    fn test_provider_no_token_warning() {
        let mut config = PipedashConfig::default();
        config.providers.insert(
            "test".to_string(),
            ProviderFileConfig {
                name: None,
                provider_type: "github".to_string(),
                token: String::new(),
                refresh_interval: 30,
                config: HashMap::new(),
            },
        );

        let result = config.validate();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.code == ConfigWarningCode::NoTokenConfigured));
    }

    #[test]
    fn test_validation_summary() {
        let result = ValidationResult::new();
        assert_eq!(result.summary(), "Configuration is valid");

        let mut result2 = ValidationResult::new();
        result2.add_error(ConfigError {
            field: "test".to_string(),
            message: "test error".to_string(),
            code: ConfigErrorCode::MissingRequired,
        });
        assert!(result2.summary().contains("1 error(s)"));
    }
}
