mod editor;
mod hierarchy;
mod members;
mod membership;
mod navigation;
mod nesting;
mod page;
mod pins;
mod snapshot;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use chrono::Utc;

use super::pipeline_service::PipelineService;
use super::presenter::Presenter;
use super::provider_service::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
    PipelineMenuSnapshot,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    ViewRef,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::{
    PipelineViewRepository,
    Repository,
};
use crate::infrastructure::ConfigState;

pub struct PipelineViewService {
    views: PipelineViewRepository,
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    pipeline_service: Arc<PipelineService>,
    config_state: Arc<ConfigState>,
    event_bus: Arc<dyn EventBus>,
    presenter: Arc<Presenter>,
    write_lock: tokio::sync::Mutex<()>,
}

impl PipelineViewService {
    pub(crate) fn new(
        repository: Arc<Repository>, provider_service: Arc<ProviderService>,
        pipeline_service: Arc<PipelineService>, config_state: Arc<ConfigState>,
        event_bus: Arc<dyn EventBus>, presenter: Arc<Presenter>,
    ) -> Self {
        Self {
            views: PipelineViewRepository::new(repository.cache_pool().clone()),
            repository,
            provider_service,
            pipeline_service,
            config_state,
            event_bus,
            presenter,
            write_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn list_views(&self) -> DomainResult<Vec<PipelineView>> {
        Ok(page::list_views(&self.snapshot().await?))
    }

    pub async fn page(&self, query: PipelineViewQuery) -> DomainResult<PipelineViewPage> {
        let mut snapshot = self.snapshot().await?;
        self.presenter.pipelines(&mut snapshot.pipelines).await?;
        page::build_page(&snapshot, &query, Utc::now())
    }

    pub async fn menu_snapshot(&self) -> DomainResult<PipelineMenuSnapshot> {
        let mut snapshot = self.snapshot().await?;
        self.presenter.pipelines(&mut snapshot.pipelines).await?;
        Ok(page::menu_snapshot(&snapshot))
    }

    async fn view(&self, id: &ViewRef) -> DomainResult<PipelineView> {
        self.list_views()
            .await?
            .into_iter()
            .find(|g| g.id == *id)
            .ok_or_else(|| DomainError::NotFound(format!("Pipeline view {id} not found")))
    }

    async fn changed(&self) {
        self.event_bus.emit(CoreEvent::PipelineViewsChanged).await;
    }
}
