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
use crate::infrastructure::{
    ConfigBackend,
    TokenStore,
};

pub struct Repository {
    config_backend: Arc<dyn ConfigBackend>,
    cache_pool: DatabasePool,
    token_store: Arc<dyn TokenStore>,
}

impl Repository {
    pub fn new(
        config_backend: Arc<dyn ConfigBackend>, cache_pool: DatabasePool,
        token_store: Arc<dyn TokenStore>,
    ) -> Self {
        Self {
            config_backend,
            cache_pool,
            token_store,
        }
    }

    pub fn cache_pool(&self) -> &DatabasePool {
        &self.cache_pool
    }

    pub fn config_backend(&self) -> &Arc<dyn ConfigBackend> {
        &self.config_backend
    }
}
