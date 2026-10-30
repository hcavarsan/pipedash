use pipedash_plugin_api::*;
use serde_json::Value;

use crate::config;
use crate::schema;

const URL_REGEX: &str = r"^https?://.+";
const URL_MESSAGE: &str = "Must be a valid HTTP or HTTPS URL";

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Prow".to_string(),
        provider_type: config::PROVIDER_TYPE.to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor, start and abort Kubernetes Prow jobs".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/prow.svg".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: Vec::new(),
        features: Vec::new(),
    }
}

fn field(
    key: &str, section: ConfigSection, label: &str, field_type: ConfigFieldType, description: &str,
) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        section,
        label: label.to_string(),
        description: Some(description.to_string()),
        field_type,
        required: false,
        default_value: None,
        options: None,
        validation_regex: None,
        validation_message: None,
    }
}

fn text(value: &str) -> Option<Value> {
    Some(Value::String(value.to_string()))
}

fn options(values: &[&str]) -> Option<Vec<String>> {
    Some(values.iter().map(|value| value.to_string()).collect())
}

fn url_field(field: ConfigField) -> ConfigField {
    ConfigField {
        validation_regex: Some(URL_REGEX.to_string()),
        validation_message: Some(URL_MESSAGE.to_string()),
        ..field
    }
}

fn create_config_schema() -> ConfigSchema {
    const TOKEN: &[AuthMode] = &[AuthMode::Token];
    const SIGN_IN: &[AuthMode] = &[AuthMode::SignIn];
    const SIGNED: &[AuthMode] = &[AuthMode::Token, AuthMode::SignIn];

    ConfigSchema::new()
        .allow_no_auth()
        .add_field(url_field(ConfigField {
            required: true,
            default_value: text(config::DEFAULT_DECK_URL),
            ..field(
                config::DECK_URL,
                ConfigSection::Connection,
                "Deck URL",
                ConfigFieldType::Text,
                "Address of Prow's Deck dashboard, for example https://prow.k8s.io",
            )
        }))
        .add_field(ConfigField {
            default_value: Some(Value::Bool(false)),
            ..field(
                config::INSECURE,
                ConfigSection::Connection,
                "Skip TLS Verification",
                ConfigFieldType::Boolean,
                "Skip TLS certificate verification (useful for self-signed certificates)",
            )
        })
        .add_auth_field(
            TOKEN,
            ConfigField {
                default_value: text("bearer"),
                options: options(&["bearer", "basic", "cookie"]),
                ..field(
                    config::AUTH_SCHEME,
                    ConfigSection::Authentication,
                    "Token type",
                    ConfigFieldType::Select,
                    "How Pipedash sends the token: bearer sends Authorization: Bearer (OAuth2 \
                     Proxy, API gateways), basic sends user:password, cookie sends a session \
                     cookie such as _oauth2_proxy=…",
                )
            },
        )
        .add_field(ConfigField {
            required: true,
            ..field(
                config::TOKEN,
                ConfigSection::Authentication,
                "Access token",
                ConfigFieldType::Password,
                "Credential for a Deck behind OAuth2 Proxy or an API gateway. Choose No auth for \
                 public Deck instances such as prow.k8s.io",
            )
        })
        .add_auth_field(
            SIGN_IN,
            url_field(ConfigField {
                required: true,
                ..field(
                    config::SSO_ISSUER,
                    ConfigSection::Authentication,
                    "Identity provider URL",
                    ConfigFieldType::Text,
                    "OpenID Connect issuer that protects Deck, for example \
                     https://dex.example.com or https://accounts.google.com",
                )
            }),
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField {
                required: true,
                ..field(
                    config::SSO_CLIENT_ID,
                    ConfigSection::Authentication,
                    "Client ID",
                    ConfigFieldType::Text,
                    "Public client registered for Pipedash with PKCE and the redirect URI below",
                )
            },
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField {
                default_value: text(config::DEFAULT_SCOPES),
                ..field(
                    config::SSO_SCOPES,
                    ConfigSection::Authentication,
                    "Scopes",
                    ConfigFieldType::Text,
                    "Space-separated scopes to request",
                )
            },
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField {
                default_value: text("id_token"),
                options: options(&["id_token", "access_token"]),
                ..field(
                    config::SSO_TOKEN,
                    ConfigSection::Authentication,
                    "Token sent to Deck",
                    ConfigFieldType::Select,
                    "id_token suits OAuth2 Proxy and most gateways; choose access_token when the \
                     gateway expects an OAuth access token",
                )
            },
        )
        .add_auth_field(
            SIGN_IN,
            url_field(ConfigField {
                default_value: text(config::DEFAULT_REDIRECT_URI),
                ..field(
                    config::SSO_REDIRECT_URI,
                    ConfigSection::Authentication,
                    "Redirect URI",
                    ConfigFieldType::Text,
                    "Must match a redirect URI of the client. An http://localhost:<port> address \
                     lets Pipedash finish the sign-in by itself",
                )
            }),
        )
        .add_auth_field(
            SIGN_IN,
            field(
                config::SSO_PARAMS,
                ConfigSection::Authentication,
                "Extra sign-in parameters (optional)",
                ConfigFieldType::Text,
                "Extra authorization parameters as key=value pairs joined by &, for example \
                 audience=deck&connector_id=github",
            ),
        )
        .add_field(ConfigField {
            default_value: text("deck"),
            options: options(&["deck", "storage"]),
            ..field(
                config::HISTORY_SOURCE,
                ConfigSection::Settings,
                "Run history",
                ConfigFieldType::Select,
                "deck shows the runs Deck still lists, usually the last 2 to 7 days. storage \
                 also reads older builds from the job's Google Cloud Storage bucket",
            )
        })
        .add_field(url_field(field(
            config::STORAGE_URL,
            ConfigSection::Settings,
            "Artifact storage URL (optional)",
            ConfigFieldType::Text,
            "Where finished build logs are read from as <url>/<bucket>/<path>. Leave empty for \
             Google Cloud Storage; set it for S3-compatible storage such as MinIO",
        )))
        .add_field(field(
            config::STORAGE_BUCKET,
            ConfigSection::Settings,
            "Storage bucket (optional)",
            ConfigFieldType::Text,
            "Bucket Prow uploads to, for example gs://kubernetes-ci-logs. Used when Deck no \
             longer lists any run of a job",
        ))
        .add_auth_field(
            SIGNED,
            field(
                config::KUBECONFIG_PATH,
                ConfigSection::Authentication,
                "Kubeconfig for actions (optional)",
                ConfigFieldType::Text,
                "Path to a kubeconfig for the cluster that runs Prow. When set, Pipedash starts \
                 and aborts jobs by creating and patching ProwJobs instead of calling Deck. \
                 Desktop and web server only",
            ),
        )
        .add_auth_field(
            SIGNED,
            ConfigField {
                options: Some(Vec::new()),
                ..field(
                    config::KUBE_CONTEXT,
                    ConfigSection::Authentication,
                    "Kubernetes context",
                    ConfigFieldType::Select,
                    "Leave empty to use the kubeconfig's current context",
                )
            },
        )
        .add_field(status_page_field())
}

fn create_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        pipelines: true,
        pipeline_runs: true,
        trigger: true,
        cancel: true,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
        retains_run_history: false,
        logs: true,
        sign_in: true,
    }
}
