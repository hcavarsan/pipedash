use chrono::{
    DateTime,
    Utc,
};

use super::{
    CorrelationConn,
    ObservationWindowRow,
    ObservedTransitionRow,
    RunEvidenceRow,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn insert_observed_transition(
        &mut self, row: &ObservedTransitionRow,
    ) -> DomainResult<i64> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_observed_transition_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_observed_transition_postgres.sql"),
        );
        let values = [
            BindValue::Text(&row.pipeline_id),
            BindValue::Int(row.provider_id),
            BindValue::Timestamp(row.at),
            BindValue::OptText(row.status_before.as_deref()),
            BindValue::Text(&row.status_after),
            BindValue::OptText(row.health_before.as_deref()),
            BindValue::OptText(row.health_after.as_deref()),
            BindValue::OptText(row.revision.as_deref()),
            BindValue::Text(&row.digests_json),
        ];
        self.insert_returning_id(sql, &values).await
    }

    pub async fn delete_observed_transitions_by_pipeline(
        &mut self, pipeline_id: &str,
    ) -> DomainResult<u64> {
        let values = [BindValue::Text(pipeline_id)];
        self.delete_where("observed_transitions", &["pipeline_id"], &values)
            .await
    }

    pub async fn delete_observed_transitions_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<u64> {
        let values = [BindValue::Int(provider_id)];
        self.delete_where("observed_transitions", &["provider_id"], &values)
            .await
    }

    pub async fn list_observed_transitions(&mut self) -> DomainResult<Vec<ObservedTransitionRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_observed_transitions_by_pipeline(
        &mut self, pipeline_id: &str,
    ) -> DomainResult<Vec<ObservedTransitionRow>> {
        self.list(&["pipeline_id"], &[BindValue::Text(pipeline_id)])
            .await
    }

    pub async fn list_observed_transitions_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<Vec<ObservedTransitionRow>> {
        self.list(&["provider_id"], &[BindValue::Int(provider_id)])
            .await
    }

    pub async fn upsert_observation_window(
        &mut self, row: &ObservationWindowRow,
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_observation_window_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_observation_window_postgres.sql"),
        );
        let values = [
            BindValue::Int(row.provider_id),
            BindValue::Timestamp(row.started_at),
            BindValue::Timestamp(row.ended_at),
        ];
        self.execute(sql, &values).await?;
        Ok(())
    }

    pub async fn delete_observation_windows_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<u64> {
        let values = [BindValue::Int(provider_id)];
        self.delete_where("observation_windows", &["provider_id"], &values)
            .await
    }

    pub async fn list_observation_windows(&mut self) -> DomainResult<Vec<ObservationWindowRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_observation_windows_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<Vec<ObservationWindowRow>> {
        self.list(&["provider_id"], &[BindValue::Int(provider_id)])
            .await
    }

    pub async fn upsert_run_evidence(&mut self, row: &RunEvidenceRow) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_run_evidence_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_run_evidence_postgres.sql"),
        );
        let values = [
            BindValue::Text(&row.pipeline_id),
            BindValue::Int(row.run_number),
            BindValue::Bool(row.outcome_recorded),
            BindValue::Bool(row.timing_recorded),
            BindValue::Bool(row.attempt),
            BindValue::Timestamp(row.started_at),
            BindValue::OptTimestamp(row.concluded_at),
            BindValue::Text(&row.status),
        ];
        self.execute(sql, &values).await?;
        Ok(())
    }

    pub async fn delete_run_evidence_by_pipeline(
        &mut self, pipeline_id: &str,
    ) -> DomainResult<u64> {
        let values = [BindValue::Text(pipeline_id)];
        self.delete_where("run_evidence", &["pipeline_id"], &values)
            .await
    }

    pub async fn get_run_evidence(
        &mut self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Option<RunEvidenceRow>> {
        let values = [BindValue::Text(pipeline_id), BindValue::Int(run_number)];
        self.get(&["pipeline_id", "run_number"], &values).await
    }

    pub async fn list_run_evidence(&mut self) -> DomainResult<Vec<RunEvidenceRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_run_evidence_by_pipeline(
        &mut self, pipeline_id: &str,
    ) -> DomainResult<Vec<RunEvidenceRow>> {
        self.list(&["pipeline_id"], &[BindValue::Text(pipeline_id)])
            .await
    }

    pub async fn list_run_evidence_concluded_between(
        &mut self, from: DateTime<Utc>, to: DateTime<Utc>,
    ) -> DomainResult<Vec<RunEvidenceRow>> {
        let sql = self.pick(
            include_str!(
                "../../../../sql/correlation/select_run_evidence_concluded_between_sqlite.sql"
            ),
            include_str!(
                "../../../../sql/correlation/select_run_evidence_concluded_between_postgres.sql"
            ),
        );
        let values = [BindValue::Timestamp(from), BindValue::Timestamp(to)];
        self.fetch(sql, &values).await
    }
}
