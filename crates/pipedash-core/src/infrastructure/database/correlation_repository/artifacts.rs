use chrono::{
    DateTime,
    Utc,
};

use super::{
    ArtifactRow,
    CorrelationConn,
    LogScanRow,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn upsert_artifact(&mut self, row: &ArtifactRow) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_artifact_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_artifact_postgres.sql"),
        );
        let values = [
            BindValue::Text(&row.digest),
            BindValue::Text(&row.repository),
            BindValue::Timestamp(row.first_seen),
            BindValue::OptText(row.label_source.as_deref()),
            BindValue::OptText(row.label_revision.as_deref()),
            BindValue::OptTimestamp(row.registry_checked_at),
            BindValue::OptTimestamp(row.log_scanned_at),
        ];
        self.execute(sql, &values).await?;
        Ok(())
    }

    pub async fn mark_artifact_registry_checked(
        &mut self, digest: &str, label_source: Option<&str>, label_revision: Option<&str>,
        at: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE artifacts SET label_source = {}, label_revision = {}, \
             registry_checked_at = {} WHERE digest = {}",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4)
        );
        let values = [
            BindValue::OptText(label_source),
            BindValue::OptText(label_revision),
            BindValue::Timestamp(at),
            BindValue::Text(digest),
        ];
        Ok(self.execute(&sql, &values).await? > 0)
    }

    pub async fn mark_artifact_log_scanned(
        &mut self, digest: &str, at: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE artifacts SET log_scanned_at = {} WHERE digest = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let values = [BindValue::Timestamp(at), BindValue::Text(digest)];
        Ok(self.execute(&sql, &values).await? > 0)
    }

    pub async fn delete_artifact(&mut self, digest: &str) -> DomainResult<bool> {
        let removed = self
            .delete_where("artifacts", &["digest"], &[BindValue::Text(digest)])
            .await?;
        Ok(removed > 0)
    }

    pub async fn get_artifact(&mut self, digest: &str) -> DomainResult<Option<ArtifactRow>> {
        self.get(&["digest"], &[BindValue::Text(digest)]).await
    }

    pub async fn list_artifacts(&mut self) -> DomainResult<Vec<ArtifactRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_artifacts_by_repository(
        &mut self, repository: &str,
    ) -> DomainResult<Vec<ArtifactRow>> {
        self.list(&["repository"], &[BindValue::Text(repository)])
            .await
    }

    pub async fn upsert_log_scan(&mut self, row: &LogScanRow) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_log_scan_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_log_scan_postgres.sql"),
        );
        let values = [
            BindValue::Text(&row.pipeline_id),
            BindValue::Int(row.run_number),
            BindValue::Text(&row.run_hash),
            BindValue::Timestamp(row.scanned_at),
        ];
        self.execute(sql, &values).await?;
        Ok(())
    }

    pub async fn delete_log_scans_by_pipeline(&mut self, pipeline_id: &str) -> DomainResult<u64> {
        let values = [BindValue::Text(pipeline_id)];
        self.delete_where("log_scans", &["pipeline_id"], &values)
            .await
    }

    pub async fn get_log_scan(
        &mut self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Option<LogScanRow>> {
        let values = [BindValue::Text(pipeline_id), BindValue::Int(run_number)];
        self.get(&["pipeline_id", "run_number"], &values).await
    }

    pub async fn list_log_scans(&mut self) -> DomainResult<Vec<LogScanRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_log_scans_by_pipeline(
        &mut self, pipeline_id: &str,
    ) -> DomainResult<Vec<LogScanRow>> {
        self.list(&["pipeline_id"], &[BindValue::Text(pipeline_id)])
            .await
    }
}
