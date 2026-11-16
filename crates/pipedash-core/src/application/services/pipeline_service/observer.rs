use async_trait::async_trait;

use crate::domain::{
    Pipeline,
    PipelineRun,
};

#[async_trait]
pub(crate) trait FetchObserver: Send + Sync {
    async fn pipelines_applied(&self, provider_id: i64, old: &[Pipeline], new: &[Pipeline]);
    async fn runs_fetched(&self, provider_id: i64, pipeline_id: &str, runs: &[PipelineRun]);
}
