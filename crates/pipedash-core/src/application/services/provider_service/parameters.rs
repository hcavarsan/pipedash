use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Mutex;

use super::ProviderService;
use crate::application::services::pipeline_service::overdue::provider_id_of;
use crate::domain::{
    DomainError,
    DomainResult,
};

struct ParameterFetch<'a> {
    fetches: &'a DashMap<String, Arc<Mutex<()>>>,
    key: &'a str,
    lock: Arc<Mutex<()>>,
}

impl<'a> ParameterFetch<'a> {
    fn new(fetches: &'a DashMap<String, Arc<Mutex<()>>>, key: &'a str) -> Self {
        let lock = Arc::clone(fetches.entry(key.to_string()).or_default().value());

        Self { fetches, key, lock }
    }
}

impl Drop for ParameterFetch<'_> {
    fn drop(&mut self) {
        self.fetches
            .remove_if(self.key, |_, lock| Arc::ptr_eq(lock, &self.lock));
    }
}

impl ProviderService {
    pub async fn get_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>> {
        let provider_id = provider_id_of(workflow_id).ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "Invalid workflow ID '{workflow_id}'. Expected format 'provider__id__...' with a numeric provider ID."
            ))
        })?;
        if let Some(cached) = self.cached_parameters(workflow_id).await {
            return Ok(cached);
        }

        let lease = self.acquire(provider_id).await?;
        let fetch = ParameterFetch::new(&self.parameter_fetches, workflow_id);
        let _guard = fetch.lock.lock().await;

        if let Some(cached) = self.cached_parameters(workflow_id).await {
            return Ok(cached);
        }

        let parameters = lease.provider.get_workflow_parameters(workflow_id).await?;

        let cached = self
            .commit(&lease, || {
                self.repository
                    .cache_workflow_parameters(workflow_id, &parameters)
            })
            .await;
        if let Some(Err(e)) = cached {
            tracing::warn!(provider_id, workflow_id, error = %e, "Failed to cache workflow parameters");
        }

        Ok(parameters)
    }

    async fn cached_parameters(
        &self, workflow_id: &str,
    ) -> Option<Vec<pipedash_plugin_api::WorkflowParameter>> {
        match self
            .repository
            .get_cached_workflow_parameters(workflow_id)
            .await
        {
            Ok(cached) => cached,
            Err(e) => {
                tracing::warn!(workflow_id, error = %e, "Failed to read cached workflow parameters");
                None
            }
        }
    }

    pub(super) async fn store_permissions(
        &self, id: i64, status: &pipedash_plugin_api::PermissionStatus,
    ) {
        if let Err(e) = self.repository.store_provider_permissions(id, status).await {
            tracing::warn!(provider_id = id, error = %e, "Failed to store permissions");
        }
    }
}
