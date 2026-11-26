use super::{
    ArtifactObservationRow,
    CorrelationConn,
    SourceObservationRow,
    TableRow,
    TargetObservationRow,
    PIPELINE_LEVEL,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    async fn pipeline_level<R: TableRow>(
        &mut self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<R>> {
        let level = BindValue::Int(PIPELINE_LEVEL);
        match provider_id {
            Some(id) => {
                self.list(&["run_number", "provider_id"], &[level, BindValue::Int(id)])
                    .await
            }
            None => self.list(&["run_number"], &[level]).await,
        }
    }

    pub async fn list_pipeline_sources(
        &mut self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<SourceObservationRow>> {
        self.pipeline_level(provider_id).await
    }

    pub async fn list_pipeline_targets(
        &mut self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<TargetObservationRow>> {
        self.pipeline_level(provider_id).await
    }

    pub async fn list_pipeline_artifacts(
        &mut self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<ArtifactObservationRow>> {
        self.pipeline_level(provider_id).await
    }
}
