mod cache;
pub(super) mod fetch_status;
mod pipeline_row;
mod pipelines;
mod providers;
mod run_reads;
mod run_writes;
#[cfg(test)]
mod tests;
mod workflow_parameters;

use std::sync::Arc;

use super::DatabasePool;
use crate::infrastructure::reload_shadow::backend::ShadowedBackend;
use crate::infrastructure::reload_shadow::tokens::ShadowedTokens;
use crate::infrastructure::reload_shadow::ReloadShadow;
use crate::infrastructure::{
    ConfigBackend,
    TokenStore,
};

pub struct Repository {
    config_backend: Arc<dyn ConfigBackend>,
    cache_pool: DatabasePool,
    token_store: Arc<dyn TokenStore>,
    shadow: Arc<ReloadShadow>,
}

impl Repository {
    pub fn new(
        config_backend: Arc<dyn ConfigBackend>, cache_pool: DatabasePool,
        token_store: Arc<dyn TokenStore>,
    ) -> Self {
        let shadow = Arc::new(ReloadShadow::default());

        Self {
            config_backend: Arc::new(ShadowedBackend::new(config_backend, Arc::clone(&shadow))),
            cache_pool,
            token_store: Arc::new(ShadowedTokens::new(token_store, Arc::clone(&shadow))),
            shadow,
        }
    }

    pub fn cache_pool(&self) -> &DatabasePool {
        &self.cache_pool
    }

    pub fn config_backend(&self) -> &Arc<dyn ConfigBackend> {
        &self.config_backend
    }

    pub(crate) fn shadow(&self) -> &Arc<ReloadShadow> {
        &self.shadow
    }
}
