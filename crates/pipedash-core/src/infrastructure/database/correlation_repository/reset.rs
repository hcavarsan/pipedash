use super::CorrelationConn;
use crate::domain::DomainResult;

const TABLES: [&str; 17] = [
    "change_stages",
    "changes",
    "application_overrides",
    "target_directions",
    "application_targets",
    "application_members",
    "applications",
    "correlation_edges",
    "artifacts",
    "log_scans",
    "observed_transitions",
    "observation_windows",
    "run_evidence",
    "artifact_observations",
    "target_observations",
    "source_observations",
    "subject_identities",
];

impl CorrelationConn {
    pub async fn clear(&mut self) -> DomainResult<()> {
        let mut scope = self.scope().await?;
        for table in TABLES {
            scope.delete_where(table, &[], &[]).await?;
        }
        scope.finish().await
    }
}
