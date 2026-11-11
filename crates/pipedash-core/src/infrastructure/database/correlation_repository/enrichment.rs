use super::{
    ArtifactRow,
    CorrelationConn,
    SourceObservationRow,
};
use crate::domain::DomainResult;

impl CorrelationConn {
    pub async fn list_unproduced_artifacts(&mut self) -> DomainResult<Vec<ArtifactRow>> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/select_unproduced_artifacts_sqlite.sql"),
            include_str!("../../../../sql/correlation/select_unproduced_artifacts_postgres.sql"),
        );
        self.fetch(sql, &[]).await
    }

    pub async fn list_unit_sources(&mut self) -> DomainResult<Vec<SourceObservationRow>> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/select_unit_sources_sqlite.sql"),
            include_str!("../../../../sql/correlation/select_unit_sources_postgres.sql"),
        );
        self.fetch(sql, &[]).await
    }
}
