use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::support::try_buffered;
use pipedash_plugin_api::support::CONCURRENCY;
use pipedash_plugin_api::*;
use tokio::sync::OnceCell;
use tracing::{
    debug,
    info,
    warn,
};

use crate::config::{
    self,
    Bucket,
    HistorySource,
    KubeSettings,
    Selection,
};
use crate::deck::DeckClient;
use crate::mapper::{
    self,
    Latest,
};
use crate::metadata;
use crate::prowjobs::ProwJobClient;
use crate::sign_in;
use crate::storage::{
    StorageClient,
    StoredBuild,
};
use crate::types::ProwJob;

mod actions;
mod catalog;
mod history;
mod logs;

const READ_ONLY: &str = "Prow is connected without credentials, which is read-only";

plugin_struct!(
    custom ProwPlugin,
    metadata::create_metadata();
    deck: Option<Arc<DeckClient>> = None,
    storage: Option<StorageClient> = None,
    history: HistorySource = HistorySource::Deck,
    storage_bucket: Option<Bucket> = None,
    selections: Vec<Selection> = Vec::new(),
    read_only: bool = false,
    kube_settings: Option<KubeSettings> = None,
    kube: OnceCell<ProwJobClient> = OnceCell::new(),
    status_source: Option<StatusSource> = None
);

impl ProwPlugin {
    fn deck(&self) -> PluginResult<&DeckClient> {
        self.deck
            .as_deref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn storage(&self) -> PluginResult<&StorageClient> {
        self.storage
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn writable(&self) -> PluginResult<()> {
        if self.read_only {
            return Err(PluginError::NotSupported(READ_ONLY.to_string()));
        }
        Ok(())
    }

    async fn kube(&self) -> PluginResult<Option<&ProwJobClient>> {
        let Some(settings) = &self.kube_settings else {
            return Ok(None);
        };
        self.kube
            .get_or_try_init(|| ProwJobClient::connect(settings))
            .await
            .map(Some)
    }
}

plugin_impl!(ProwPlugin [
    metadata,
    status_source,
    available_from_filtered,
    fetch_available_pipelines_filtered,
    fetch_run_history,
    fetch_recent_runs,
    fetch_run_details,
    fetch_workflow_parameters,
    trigger_pipeline,
    cancel_run,
    fetch_log_nodes,
    read_log,
] {
    fn configured_repositories(&self, _config: &HashMap<String, String>) -> Vec<String> {
        Vec::new()
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions {
            origin: config::deck_url(config).ok(),
            accept_invalid_certs: config::is_insecure(config),
            ..ClientOptions::default()
        }
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing Prow plugin");

        let status_source = custom_status_source(&config)?;
        let base = config::deck_url(&config)?;
        let read_only = is_no_auth(&config);
        let auth = if read_only {
            None
        } else {
            config::auth_header(&config)?
        };
        let history = config::history_source(&config)?;
        let storage_url = config::storage_url(&config)?;
        let storage_bucket = config::storage_bucket(&config)?;
        let selections = config::selections(&config)?;
        let kube_settings = config::kube_settings(&config).filter(|_| !read_only);
        if config::is_insecure(&config) {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }
        let http_client = require_client(http_client)?;

        let deck = Arc::new(DeckClient::new(Arc::clone(&http_client), base, auth));
        self.storage = Some(StorageClient::new(
            http_client,
            Arc::clone(&deck),
            storage_url,
        ));
        self.deck = Some(deck);
        self.history = history;
        self.storage_bucket = storage_bucket;
        self.selections = selections;
        self.read_only = read_only;
        self.kube_settings = kube_settings;
        self.kube = OnceCell::new();
        self.status_source = status_source;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.deck()?.check_access().await?;
        debug!("Prow Deck reachable");
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        Ok(self.fetch_pipeline_catalog().await?.organizations)
    }

    async fn fetch_pipeline_catalog(&self) -> PluginResult<PipelineCatalog> {
        let deck = self.deck()?;
        let jobs = deck.jobs(None, None).await?;
        Ok(mapper::catalog(&jobs, deck.host()))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        if self.selections.is_empty() {
            return Ok(Vec::new());
        }
        let deck = self.deck()?;

        let jobs_by_scope = self
            .jobs_by_scope(
                self.selections
                    .iter()
                    .map(|selection| selection.scope.as_ref()),
            )
            .await?;
        let newest: Vec<Option<&ProwJob>> = self
            .selections
            .iter()
            .map(|selection| {
                jobs_by_scope
                    .get(&selection.scope.as_ref())
                    .and_then(|jobs| mapper::newest(jobs, &selection.job))
            })
            .collect();

        let stored: Vec<Option<StoredBuild>> = if self.history == HistorySource::Storage {
            let lookups: Vec<_> = self
                .selections
                .iter()
                .zip(&newest)
                .map(|(selection, newest)| async move {
                    match newest {
                        Some(_) => Ok(None),
                        None => self.stored_latest(&selection.job).await,
                    }
                })
                .collect();
            try_buffered(lookups, CONCURRENCY).await?
        } else {
            vec![None; self.selections.len()]
        };

        let pipelines: Vec<Pipeline> = self
            .selections
            .iter()
            .zip(newest)
            .zip(&stored)
            .map(|((selection, newest), stored)| {
                let latest = match (newest, stored) {
                    (Some(job), _) => Latest::Deck(job),
                    (None, Some(build)) => Latest::Storage(build),
                    (None, None) => Latest::None,
                };
                mapper::map_pipeline(selection, latest, deck.base(), self.read_only)
            })
            .collect();

        info!(count = pipelines.len(), "Mapped Prow jobs");
        Ok(pipelines)
    }

    async fn get_field_options(
        &self, field_key: &str, config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        if field_key != config::KUBE_CONTEXT {
            return Ok(Vec::new());
        }
        let Some(settings) = config::kube_settings(config) else {
            return Ok(Vec::new());
        };
        match ProwJobClient::contexts(settings.paths).await {
            Ok(contexts) => Ok(contexts),
            Err(error) => {
                warn!(%error, "Couldn't read kubeconfig contexts");
                Ok(Vec::new())
            }
        }
    }

    async fn sign_in_client(
        &self, config: &HashMap<String, String>, _http_client: &reqwest::Client,
    ) -> PluginResult<SignInClient> {
        sign_in::client_for(config)
    }
});

#[cfg(test)]
mod tests;
