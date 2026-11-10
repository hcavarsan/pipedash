use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use pipedash_plugin_api::defaults::{
    default_pipeline_runs_table,
    default_pipelines_table,
    table_schema,
};
use pipedash_plugin_api::plugin::defaults::pipeline_actions;
use pipedash_plugin_api::{
    http,
    AuthMode,
    ClientOptions,
    ConfigField,
    ConfigSchema,
    ConfigSection,
    PaginatedAvailablePipelines,
    PaginatedResponse,
    PaginationParams,
    PermissionCheck,
    PermissionStatus,
    Pipeline,
    PipelineActionState,
    PipelineFacts,
    PipelineRun,
    PipelineStatus,
    Plugin,
    PluginCapabilities,
    PluginError,
    PluginManifest,
    PluginMetadata,
    PluginRegistry,
    PluginResult,
    RerunStrategy,
    RunHistoryOrder,
    RunHistoryPolicy,
    SignInClient,
    SignInRefreshRejection,
    SignInToken,
    TriggerParams,
    TriggerResult,
};

pub(crate) const STUB: &str = "stub";
pub(crate) const STUB_BOUNDED: &str = "stub-bounded";
pub(crate) const STUB_RETAINING: &str = "stub-retaining";
pub(crate) const STUB_FORCE: &str = "stub-force";
pub(crate) const STUB_PREFIXED: &str = "stub-prefixed";
pub(crate) const STUB_TOKEN_PREFIXES: [&str; 2] = ["stubtok_", "stub-pat-"];
pub(crate) const STUB_OPTIONS: &str = "stub-options";
pub(crate) const STUB_HTTP: &str = "stub-http";
pub(crate) const STUB_OIDC: &str = "stub-oidc";
pub(crate) const STUB_DRAFT: &str = "stub-draft";

const SIGN_IN_REDIRECT: &str = "http://localhost:8085/auth/callback";
const REFRESH_REJECTIONS: &[SignInRefreshRejection] = &[
    SignInRefreshRejection {
        status: 400,
        error: "invalid_request",
        description: "Refresh token is invalid or has already been claimed by another client.",
    },
    SignInRefreshRejection {
        status: 400,
        error: "invalid_request",
        description: "Refresh token expired.",
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    Options,
    Http,
    Oidc,
}

pub(crate) struct StubPlugin {
    metadata: PluginMetadata,
    kind: Kind,
    server: Option<(reqwest::Url, Arc<reqwest::Client>)>,
}

impl StubPlugin {
    fn new() -> Self {
        let capabilities = PluginCapabilities {
            ..PluginCapabilities::standard()
        };
        Self {
            metadata: PluginMetadata::new(
                "Stub",
                ConfigSchema::new().allow_no_auth(),
                table_schema(default_pipeline_runs_table(), default_pipelines_table()),
                capabilities,
            ),
            kind: Kind::Plain,
            server: None,
        }
    }

    fn bounded() -> Self {
        let mut plugin = Self::new();
        plugin.metadata.capabilities.retains_run_history = true;
        plugin.metadata.run_history = RunHistoryPolicy {
            bounded: true,
            order: RunHistoryOrder::StartedAt,
            tracks_overdue: false,
        };
        plugin
    }

    fn prefixed() -> Self {
        let mut plugin = Self::new();
        plugin.metadata.token_prefixes = STUB_TOKEN_PREFIXES.map(str::to_string).to_vec();
        plugin
    }

    fn force() -> Self {
        let mut plugin = Self::new();
        plugin.metadata.capabilities.force_cancel = true;
        plugin
    }

    fn retaining() -> Self {
        let mut plugin = Self::new();
        plugin.metadata.capabilities.retains_run_history = true;
        plugin
    }

    fn drafting() -> Self {
        let mut plugin = Self::new();
        let text = |key: &str, section, label: &str| ConfigField::text(key, section, label);
        plugin.metadata.config_schema = ConfigSchema::new()
            .allow_no_auth()
            .add_field(
                text("deck_url", ConfigSection::Connection, "Deck URL")
                    .required()
                    .validation("^https://", "Use https"),
            )
            .add_field(text("note", ConfigSection::Settings, "Note"))
            .add_field(
                text("pattern", ConfigSection::Connection, "Pattern").validation("^https://", ""),
            )
            .add_field(
                text("broken", ConfigSection::Connection, "Broken").validation("(", "unused"),
            )
            .add_field(
                text("huge", ConfigSection::Connection, "Huge")
                    .validation("(((a{100}){100}){100}){100}", "unused"),
            )
            .add_field(text("kubeconfig", ConfigSection::Connection, "Kubeconfig"))
            .add_field(text("context", ConfigSection::Connection, "Context"))
            .add_field(text("namespaces", ConfigSection::Settings, "Namespaces"))
            .add_field(ConfigField::token("Token", "Access token"))
            .add_auth_field(
                &[AuthMode::SignIn],
                text("sso_issuer", ConfigSection::Authentication, "SSO Issuer").required(),
            )
            .add_auth_field(
                &[AuthMode::Token, AuthMode::SignIn],
                text("auth_scheme", ConfigSection::Authentication, "Auth Scheme"),
            );
        plugin
    }

    fn of(kind: Kind) -> Self {
        Self {
            kind,
            ..Self::new()
        }
    }

    async fn probe_permissions(&self) -> PluginResult<()> {
        let Some((url, client)) = self.server.as_ref().filter(|_| self.kind == Kind::Http) else {
            return Ok(());
        };
        http::check_response(http::send(url, client.get(url.clone())).await?).await?;
        Ok(())
    }

    async fn probe(&self) -> PluginResult<()> {
        let Some((url, client)) = &self.server else {
            return Ok(());
        };
        http::send(url, client.get(url.clone())).await?;
        Ok(())
    }
}

#[async_trait]
impl Plugin for StubPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn selection_required(&self, config: &HashMap<String, String>) -> bool {
        config.get("note").is_none_or(|value| value != "automatic")
    }

    fn pipeline_actions(&self, pipeline: &PipelineFacts<'_>) -> PipelineActionState {
        let mut state = pipeline_actions(&self.metadata.capabilities, pipeline);
        if pipeline.metadata.get("rerun_in_place") == Some(&serde_json::Value::Bool(true)) {
            state.rerun_strategy = RerunStrategy::InPlace;
        }
        state
    }

    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        match self.kind {
            Kind::Options if !config.contains_key("required") => Err(PluginError::InvalidConfig(
                "required is missing".to_string(),
            )),
            Kind::Http | Kind::Oidc => {
                self.server = Some((server_url(&config)?, http_client.unwrap()));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions {
            accept_invalid_certs: config.get("insecure").is_some_and(|value| value == "true"),
            ..ClientOptions::default()
        }
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.probe().await?;
        Ok(true)
    }

    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        self.probe_permissions().await?;
        Ok(PermissionStatus {
            permissions: self
                .metadata
                .required_permissions
                .iter()
                .map(|permission| PermissionCheck {
                    permission: permission.clone(),
                    granted: true,
                })
                .collect(),
            all_granted: true,
            checked_at: chrono::Utc::now(),
            metadata: HashMap::new(),
            credential_label: None,
        })
    }

    async fn fetch_available_pipelines(
        &self, _params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedAvailablePipelines> {
        self.probe().await?;
        Ok(PaginatedResponse::empty())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        Ok(Vec::new())
    }

    async fn fetch_run_history(
        &self, _pipeline_id: &str, _limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        Ok(Vec::new())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, _run_number: i64,
    ) -> PluginResult<PipelineRun> {
        Err(PluginError::PipelineNotFound(pipeline_id.to_string()))
    }

    async fn trigger_pipeline(&self, _params: TriggerParams) -> PluginResult<TriggerResult> {
        Ok(TriggerResult::default())
    }

    fn can_archive_run(
        &self, _pipeline_metadata: &HashMap<String, serde_json::Value>,
        run_metadata: &HashMap<String, serde_json::Value>, status: PipelineStatus,
    ) -> bool {
        !matches!(status, PipelineStatus::Running | PipelineStatus::Pending)
            && run_metadata.get("archivable") == Some(&serde_json::Value::Bool(true))
    }

    async fn get_field_options(
        &self, field_key: &str, _config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        match (self.kind, field_key) {
            (Kind::Options, "broken") => Err(PluginError::ApiError("options unavailable".into())),
            (Kind::Options, _) => Ok(vec!["option-a".to_string(), "option-b".to_string()]),
            _ => Ok(Vec::new()),
        }
    }

    async fn sign_in_client(
        &self, config: &HashMap<String, String>, http_client: &reqwest::Client,
    ) -> PluginResult<SignInClient> {
        if self.kind != Kind::Oidc {
            return Err(PluginError::NotSupported("sign-in".to_string()));
        }
        let url = server_url(config)?;
        let settings: serde_json::Value = http::send(&url, http_client.get(url.clone()))
            .await?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(e.to_string()))?;
        let text = |key: &str| settings[key].as_str().unwrap_or_default().to_string();
        Ok(SignInClient {
            label: "SSO".to_string(),
            issuer: text("issuer"),
            client_id: text("client_id"),
            scopes: vec!["openid".to_string(), "email".to_string()],
            redirect_uri: SIGN_IN_REDIRECT.to_string(),
            authorize_params: Vec::new(),
            token: SignInToken::IdToken,
        })
    }

    fn sign_in_refresh_rejections(&self) -> &'static [SignInRefreshRejection] {
        if self.kind == Kind::Oidc {
            REFRESH_REJECTIONS
        } else {
            &[]
        }
    }
}

pub(crate) const STUB_LOCAL: &str = "stub-local";

pub(crate) fn manifest(provider_type: &'static str) -> PluginManifest {
    PluginManifest {
        package_name: provider_type,
        version: "0.0.0",
        description: "Stub provider",
        authors: "Pipedash Team",
        icon_svg: "<svg/>",
    }
}

pub(crate) fn local_plugin() -> Box<dyn Plugin> {
    Box::new(StubPlugin::new())
}

pub(crate) fn registry() -> Arc<PluginRegistry> {
    let mut registry = PluginRegistry::new();
    registry.register(|| Box::new(StubPlugin::new()), manifest(STUB));
    registry.register(|| Box::new(StubPlugin::bounded()), manifest(STUB_BOUNDED));
    registry.register(|| Box::new(StubPlugin::prefixed()), manifest(STUB_PREFIXED));
    registry.register(
        || Box::new(StubPlugin::retaining()),
        manifest(STUB_RETAINING),
    );
    registry.register(|| Box::new(StubPlugin::force()), manifest(STUB_FORCE));
    registry.register(
        || Box::new(StubPlugin::of(Kind::Options)),
        manifest(STUB_OPTIONS),
    );
    registry.register(|| Box::new(StubPlugin::of(Kind::Http)), manifest(STUB_HTTP));
    registry.register(|| Box::new(StubPlugin::of(Kind::Oidc)), manifest(STUB_OIDC));
    registry.register(|| Box::new(StubPlugin::drafting()), manifest(STUB_DRAFT));
    Arc::new(registry)
}

fn server_url(config: &HashMap<String, String>) -> PluginResult<reqwest::Url> {
    config
        .get("server_url")
        .and_then(|url| url.parse().ok())
        .ok_or_else(|| PluginError::InvalidConfig("server_url is required".to_string()))
}
