use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

const URL_REGEX: &str = r"^https?://.+";
const URL_MESSAGE: &str = "Must be a valid HTTP or HTTPS URL";

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Prow",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities {
            sign_in: true,
            ..PluginCapabilities::standard()
        },
    )
}

fn create_config_schema() -> ConfigSchema {
    const TOKEN: &[AuthMode] = &[AuthMode::Token];
    const SIGN_IN: &[AuthMode] = &[AuthMode::SignIn];
    const SIGNED: &[AuthMode] = &[AuthMode::Token, AuthMode::SignIn];

    ConfigSchema::new()
        .allow_no_auth()
        .add_field(
            ConfigField::text(config::DECK_URL, ConfigSection::Connection, "Deck URL")
                .description("Address of Prow's Deck dashboard, for example https://prow.k8s.io")
                .required()
                .default_text(config::DEFAULT_DECK_URL)
                .validation(URL_REGEX, URL_MESSAGE),
        )
        .add_field(ConfigField::insecure_tls())
        .add_auth_field(
            TOKEN,
            ConfigField::select(
                config::AUTH_SCHEME,
                ConfigSection::Authentication,
                "Token type",
                &["bearer", "basic", "cookie"],
            )
            .description(
                "How Pipedash sends the token: bearer sends Authorization: Bearer (OAuth2 \
                 Proxy, API gateways), basic sends user:password, cookie sends a session \
                 cookie such as _oauth2_proxy=…",
            )
            .default_text("bearer"),
        )
        .add_field(ConfigField::token(
            "Access token",
            "Credential for a Deck behind OAuth2 Proxy or an API gateway. Choose No auth for \
             public Deck instances such as prow.k8s.io",
        ))
        .add_auth_field(
            SIGN_IN,
            ConfigField::text(
                config::SSO_ISSUER,
                ConfigSection::Authentication,
                "Identity provider URL",
            )
            .description(
                "OpenID Connect issuer that protects Deck, for example \
                 https://dex.example.com or https://accounts.google.com",
            )
            .required()
            .validation(URL_REGEX, URL_MESSAGE),
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField::text(
                config::SSO_CLIENT_ID,
                ConfigSection::Authentication,
                "Client ID",
            )
            .description(
                "Public client registered for Pipedash with PKCE and the redirect URI below",
            )
            .required(),
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField::text(config::SSO_SCOPES, ConfigSection::Authentication, "Scopes")
                .description("Space-separated scopes to request")
                .default_text(config::DEFAULT_SCOPES),
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField::select(
                config::SSO_TOKEN,
                ConfigSection::Authentication,
                "Token sent to Deck",
                &["id_token", "access_token"],
            )
            .description(
                "id_token suits OAuth2 Proxy and most gateways; choose access_token when the \
                 gateway expects an OAuth access token",
            )
            .default_text("id_token"),
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField::text(
                config::SSO_REDIRECT_URI,
                ConfigSection::Authentication,
                "Redirect URI",
            )
            .description(
                "Must match a redirect URI of the client. An http://localhost:<port> address \
                 lets Pipedash finish the sign-in by itself",
            )
            .default_text(config::DEFAULT_REDIRECT_URI)
            .validation(URL_REGEX, URL_MESSAGE),
        )
        .add_auth_field(
            SIGN_IN,
            ConfigField::text(
                config::SSO_PARAMS,
                ConfigSection::Authentication,
                "Extra sign-in parameters (optional)",
            )
            .description(
                "Extra authorization parameters as key=value pairs joined by &, for example \
                 audience=deck&connector_id=github",
            ),
        )
        .add_field(
            ConfigField::select(
                config::HISTORY_SOURCE,
                ConfigSection::Settings,
                "Run history",
                &["deck", "storage"],
            )
            .description(
                "deck shows the runs Deck still lists, usually the last 2 to 7 days. storage \
                 also reads older builds from the job's Google Cloud Storage bucket",
            )
            .default_text("deck"),
        )
        .add_field(
            ConfigField::text(
                config::STORAGE_URL,
                ConfigSection::Settings,
                "Artifact storage URL (optional)",
            )
            .description(
                "Where finished build logs are read from as <url>/<bucket>/<path>. Leave empty for \
                 Google Cloud Storage; set it for S3-compatible storage such as MinIO",
            )
            .validation(URL_REGEX, URL_MESSAGE),
        )
        .add_field(
            ConfigField::text(
                config::STORAGE_BUCKET,
                ConfigSection::Settings,
                "Storage bucket (optional)",
            )
            .description(
                "Bucket Prow uploads to, for example gs://kubernetes-ci-logs. Used when Deck no \
                 longer lists any run of a job",
            ),
        )
        .add_auth_field(
            SIGNED,
            ConfigField::text(
                config::KUBECONFIG_PATH,
                ConfigSection::Authentication,
                "Kubeconfig for actions (optional)",
            )
            .description(
                "Path to a kubeconfig for the cluster that runs Prow. When set, Pipedash starts \
                 and aborts jobs by creating and patching ProwJobs instead of calling Deck. \
                 Desktop and web server only",
            ),
        )
        .add_auth_field(
            SIGNED,
            ConfigField::select(
                config::KUBE_CONTEXT,
                ConfigSection::Authentication,
                "Kubernetes context",
                &[],
            )
            .description("Leave empty to use the kubeconfig's current context"),
        )
        .add_field(status_page_field())
}
