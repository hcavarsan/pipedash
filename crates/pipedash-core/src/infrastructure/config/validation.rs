use indexmap::IndexMap;
use pipedash_plugin_api::PluginRegistry;

use super::schema::{
    PipedashConfig,
    ProviderFileConfig,
    StorageBackend,
    ViewFileConfig,
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

    pub fn add_error(&mut self, error: ConfigError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: ConfigWarning) {
        self.warnings.push(warning);
    }
}

#[derive(Debug, Clone)]
pub struct ConfigError {
    pub field: String,
    pub message: String,
    pub code: ConfigErrorCode,
}

macro_rules! display_entry {
    ($entry:ty) => {
        impl std::fmt::Display for $entry {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}: {} ({})", self.field, self.message, self.code)
            }
        }
    };
}

display_entry!(ConfigError);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigErrorCode {
    MissingRequired,
    InvalidValue,
    PlainTextToken,
    FeatureNotEnabled,
}

impl std::fmt::Display for ConfigErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequired => write!(f, "MISSING_REQUIRED"),
            Self::InvalidValue => write!(f, "INVALID_VALUE"),
            Self::PlainTextToken => write!(f, "PLAIN_TEXT_TOKEN"),
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

display_entry!(ConfigWarning);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWarningCode {
    NoTokenConfigured,
    UnusedSetting,
}

impl std::fmt::Display for ConfigWarningCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTokenConfigured => write!(f, "NO_TOKEN"),
            Self::UnusedSetting => write!(f, "UNUSED"),
        }
    }
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate_with_plugins(
        config: &PipedashConfig, registry: &PluginRegistry,
    ) -> ValidationResult {
        let mut result = Self::validate(config);
        Self::validate_provider_types(&config.providers, registry, &mut result);
        result
    }

    fn validate(config: &PipedashConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        Self::validate_storage(&config.storage, &mut result);
        Self::validate_providers(&config.providers, &mut result);
        Self::validate_views(&config.views, &mut result);

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

    fn validate_views(views: &IndexMap<String, ViewFileConfig>, result: &mut ValidationResult) {
        for (key, view) in views {
            let prefix = format!("views.{key}");

            if key.is_empty() {
                result.add_error(ConfigError {
                    field: "views".to_string(),
                    message: "View ID (table key) cannot be empty".to_string(),
                    code: ConfigErrorCode::MissingRequired,
                });
            }

            if view.name.trim().is_empty() {
                result.add_error(ConfigError {
                    field: format!("{prefix}.name"),
                    message: "View name is required".to_string(),
                    code: ConfigErrorCode::MissingRequired,
                });
            }

            if view.providers.is_empty()
                && view.repositories.is_empty()
                && view
                    .name_contains
                    .as_deref()
                    .is_none_or(|s| s.trim().is_empty())
                && view.statuses.is_empty()
            {
                result.add_error(ConfigError {
                    field: prefix.clone(),
                    message: "Config view needs at least one of providers, repositories, name_contains or statuses".to_string(),
                    code: ConfigErrorCode::MissingRequired,
                });
            }

            for pattern in &view.repositories {
                if let Err(e) = glob::Pattern::new(pattern.trim()) {
                    result.add_error(ConfigError {
                        field: format!("{prefix}.repositories"),
                        message: format!("Invalid repository pattern \"{pattern}\": {e}"),
                        code: ConfigErrorCode::InvalidValue,
                    });
                }
            }
        }
    }
}

impl ConfigValidator {
    fn validate_provider_types(
        providers: &IndexMap<String, ProviderFileConfig>, registry: &PluginRegistry,
        result: &mut ValidationResult,
    ) {
        for (id, provider) in providers {
            let provider_type = provider.provider_type.as_str();
            if provider_type.is_empty() || registry.is_registered(provider_type) {
                continue;
            }
            let mut valid_types = registry.provider_types();
            valid_types.sort();
            result.add_warning(ConfigWarning {
                field: format!("providers.{}.type", id),
                message: format!(
                    "Unknown provider type: '{}'. Valid types: {:?}",
                    provider_type, valid_types
                ),
                code: ConfigWarningCode::UnusedSetting,
            });
        }
    }
}

#[cfg(test)]
mod tests;
