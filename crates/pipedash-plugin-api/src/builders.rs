use serde_json::Value;

use crate::history::RunHistoryPolicy;
use crate::metadata::{
    PluginCapabilities,
    PluginMetadata,
};
use crate::presentation::PluginPresentation;
use crate::schema::{
    AuthMode,
    AuthOption,
    ConfigField,
    ConfigFieldType,
    ConfigSchema,
    ConfigSection,
    TableSchema,
};
use crate::types::{
    Feature,
    Permission,
};

const HTTP_URL_REGEX: &str = r"^https?://.*";
const HTTP_URL_MESSAGE: &str = "Must be a valid HTTP or HTTPS URL";
const KUBECONFIG_CONTENT_DESCRIPTION: &str = "Paste the kubeconfig YAML instead of using a file path, for example on iOS and Android where no kubeconfig file exists. When set, it is used instead of the path above and is stored with your other credentials. Use token or client certificate authentication: exec credential plugins (aws, gke-gcloud-auth-plugin, kubelogin) are not supported on iOS and Android.";

impl ConfigField {
    fn with_type(
        key: &str, section: ConfigSection, label: &str, field_type: ConfigFieldType,
    ) -> Self {
        Self {
            key: key.to_string(),
            section,
            label: label.to_string(),
            description: None,
            field_type,
            required: false,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
            auth_modes: Vec::new(),
        }
    }

    pub fn text(key: &str, section: ConfigSection, label: &str) -> Self {
        Self::with_type(key, section, label, ConfigFieldType::Text)
    }

    pub fn textarea(key: &str, section: ConfigSection, label: &str) -> Self {
        Self::with_type(key, section, label, ConfigFieldType::TextArea)
    }

    pub fn password(key: &str, section: ConfigSection, label: &str) -> Self {
        Self::with_type(key, section, label, ConfigFieldType::Password)
    }

    pub fn boolean(key: &str, section: ConfigSection, label: &str) -> Self {
        Self::with_type(key, section, label, ConfigFieldType::Boolean)
    }

    pub fn select(key: &str, section: ConfigSection, label: &str, options: &[&str]) -> Self {
        Self {
            options: Some(options.iter().map(|option| option.to_string()).collect()),
            ..Self::with_type(key, section, label, ConfigFieldType::Select)
        }
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn default_text(mut self, value: &str) -> Self {
        self.default_value = Some(Value::String(value.to_string()));
        self
    }

    pub fn default_bool(mut self, value: bool) -> Self {
        self.default_value = Some(Value::Bool(value));
        self
    }

    pub fn validation(mut self, regex: &str, message: &str) -> Self {
        self.validation_regex = Some(regex.to_string());
        self.validation_message = Some(message.to_string());
        self
    }

    pub fn http_url_validation(self) -> Self {
        self.validation(HTTP_URL_REGEX, HTTP_URL_MESSAGE)
    }

    pub fn token(label: &str, description: &str) -> Self {
        Self::password("token", ConfigSection::Authentication, label)
            .description(description)
            .required()
    }

    pub fn insecure_tls() -> Self {
        Self::boolean(
            "insecure",
            ConfigSection::Connection,
            "Skip TLS Verification",
        )
        .description("Skip TLS certificate verification (useful for self-signed certificates)")
        .default_bool(false)
    }

    pub fn kubeconfig_content(key: &str, label: &str) -> Self {
        Self::textarea(key, ConfigSection::Authentication, label)
            .description(KUBECONFIG_CONTENT_DESCRIPTION)
    }
}

impl AuthOption {
    fn new(mode: AuthMode, label: &str, description: &str) -> Self {
        Self {
            mode,
            label: label.to_string(),
            description: description.to_string(),
        }
    }

    pub fn none() -> Self {
        Self::new(
            AuthMode::None,
            "No auth",
            "Connects without credentials. Read-only: starting and stopping runs is turned off.",
        )
    }

    pub fn token(label: &str, description: &str) -> Self {
        Self::new(AuthMode::Token, label, description)
    }

    pub fn sign_in(description: &str) -> Self {
        Self::new(AuthMode::SignIn, "Single sign-on", description)
    }

    pub fn kubeconfig(description: &str) -> Self {
        Self::new(AuthMode::Kubeconfig, "Kubernetes", description)
    }
}

impl Permission {
    pub fn new(name: &str, description: &str, required: bool) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            required,
        }
    }
}

impl Feature {
    pub fn new(id: &str, name: &str, description: &str, required_permissions: &[&str]) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            required_permissions: required_permissions
                .iter()
                .map(|permission| permission.to_string())
                .collect(),
        }
    }
}

impl PluginCapabilities {
    pub fn standard() -> Self {
        Self {
            trigger: true,
            cancel: true,
            logs: true,
            uses_http_proxy: true,
            ..Self::default()
        }
    }
}

impl PluginMetadata {
    pub fn new(
        name: &str, config_schema: ConfigSchema, table_schema: TableSchema,
        capabilities: PluginCapabilities,
    ) -> Self {
        Self {
            name: name.to_string(),
            provider_type: String::new(),
            version: String::new(),
            description: String::new(),
            author: None,
            icon: None,
            config_schema,
            table_schema,
            capabilities,
            required_permissions: Vec::new(),
            features: Vec::new(),
            run_history: RunHistoryPolicy::default(),
            token_prefixes: Vec::new(),
            presentation: PluginPresentation::new(name),
        }
    }

    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.required_permissions = permissions;
        self
    }

    pub fn with_features(mut self, features: Vec<Feature>) -> Self {
        self.features = features;
        self
    }
}

#[cfg(test)]
mod tests;
