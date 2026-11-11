use super::{
    ChangeRow,
    ChangeStageRow,
    CorrelationConn,
    TableRow,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn upsert_change(&mut self, row: &ChangeRow) -> DomainResult<i64> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_change_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_change_postgres.sql"),
        );
        let values = [
            BindValue::Int(row.application_id),
            BindValue::Text(&row.repo),
            BindValue::Text(&row.commit_sha),
            BindValue::OptTimestamp(row.authored_at),
        ];
        self.insert_returning_id(sql, &values).await
    }

    pub async fn delete_change(&mut self, id: i64) -> DomainResult<bool> {
        let removed = self
            .delete_where("changes", &["id"], &[BindValue::Int(id)])
            .await?;
        Ok(removed > 0)
    }

    pub async fn delete_changes_by_application(
        &mut self, application_id: i64,
    ) -> DomainResult<u64> {
        let values = [BindValue::Int(application_id)];
        self.delete_where("changes", &["application_id"], &values)
            .await
    }

    pub async fn get_change(&mut self, id: i64) -> DomainResult<Option<ChangeRow>> {
        self.get(&["id"], &[BindValue::Int(id)]).await
    }

    pub async fn get_change_by_commit(
        &mut self, application_id: i64, repo: &str, commit_sha: &str,
    ) -> DomainResult<Option<ChangeRow>> {
        let values = [
            BindValue::Int(application_id),
            BindValue::Text(repo),
            BindValue::Text(commit_sha),
        ];
        self.get(&["application_id", "repo", "commit_sha"], &values)
            .await
    }

    pub async fn list_changes(&mut self) -> DomainResult<Vec<ChangeRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_changes_by_application(
        &mut self, application_id: i64,
    ) -> DomainResult<Vec<ChangeRow>> {
        self.list(&["application_id"], &[BindValue::Int(application_id)])
            .await
    }

    pub async fn list_changes_page(
        &mut self, application_id: i64, limit: i64, offset: i64,
    ) -> DomainResult<Vec<ChangeRow>> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/select_changes_page_sqlite.sql"),
            include_str!("../../../../sql/correlation/select_changes_page_postgres.sql"),
        );
        let values = [
            BindValue::Int(application_id),
            BindValue::Int(limit),
            BindValue::Int(offset),
        ];
        self.fetch(sql, &values).await
    }

    pub async fn list_changes_by_commit(
        &mut self, repo: &str, commit_sha: &str,
    ) -> DomainResult<Vec<ChangeRow>> {
        let values = [BindValue::Text(repo), BindValue::Text(commit_sha)];
        self.list(&["repo", "commit_sha"], &values).await
    }

    pub async fn replace_change_stages(
        &mut self, change_id: i64, rows: &[ChangeStageRow],
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_change_stage_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_change_stage_postgres.sql"),
        );
        let mut scope = self.scope().await?;
        scope
            .delete_where(
                "change_stages",
                &["change_id"],
                &[BindValue::Int(change_id)],
            )
            .await?;
        for row in rows {
            let values = [
                BindValue::Int(change_id),
                BindValue::Text(&row.stage),
                BindValue::Text(&row.group_key),
                BindValue::Text(&row.pipeline_id),
                BindValue::Int(row.run_number),
                BindValue::Timestamp(row.started_at),
                BindValue::OptTimestamp(row.concluded_at),
                BindValue::Text(&row.status),
                BindValue::Bool(row.outcome_recorded),
                BindValue::Bool(row.timing_recorded),
                BindValue::Bool(row.attempt),
            ];
            scope.execute(sql, &values).await?;
        }
        scope.finish().await
    }

    pub async fn list_change_stages(&mut self) -> DomainResult<Vec<ChangeStageRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_change_stages_by_change(
        &mut self, change_id: i64,
    ) -> DomainResult<Vec<ChangeStageRow>> {
        self.list(&["change_id"], &[BindValue::Int(change_id)])
            .await
    }

    pub async fn list_change_stages_by_application(
        &mut self, application_id: i64,
    ) -> DomainResult<Vec<ChangeStageRow>> {
        let sql = self.pick(
            include_str!(
                "../../../../sql/correlation/select_change_stages_for_application_sqlite.sql"
            ),
            include_str!(
                "../../../../sql/correlation/select_change_stages_for_application_postgres.sql"
            ),
        );
        self.fetch(sql, &[BindValue::Int(application_id)]).await
    }

    pub async fn list_change_stages_by_pipeline(
        &mut self, pipeline_id: &str,
    ) -> DomainResult<Vec<ChangeStageRow>> {
        self.list(&["pipeline_id"], &[BindValue::Text(pipeline_id)])
            .await
    }

    pub async fn list_changes_without_authored_at(
        &mut self, limit: i64,
    ) -> DomainResult<Vec<ChangeRow>> {
        let sql = format!(
            "SELECT {} FROM changes WHERE authored_at IS NULL ORDER BY id LIMIT {}",
            ChangeRow::COLUMNS,
            self.placeholder(1)
        );
        self.fetch(&sql, &[BindValue::Int(limit)]).await
    }
}
