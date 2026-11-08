use crate::application::PipelineService;
use crate::domain::provider_draft::ProviderDraft;
use crate::domain::{
    DomainResult,
    RefreshState,
};
use crate::CoreContext;

impl CoreContext {
    pub(super) async fn refresh_new_provider(&self, id: i64) {
        let results = self
            .pipeline_service
            .fetch_now(&[id], PipelineService::REFRESH_DEADLINE)
            .await;
        if let Some(result) = results
            .iter()
            .find(|result| result.state == RefreshState::Failed)
        {
            tracing::warn!(
                provider_id = id,
                kind = ?result.kind,
                message = ?result.message,
                "Initial fetch for new provider failed"
            );
        }
    }

    pub async fn save_provider_draft(&self, draft: ProviderDraft) -> DomainResult<i64> {
        let is_new = draft.provider_id.is_none();
        let id = self.provider_service.save_draft(draft).await?;
        if is_new {
            self.refresh_new_provider(id).await;
        }

        Ok(id)
    }
}
