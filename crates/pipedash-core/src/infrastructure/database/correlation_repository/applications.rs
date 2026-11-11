use chrono::{
    DateTime,
    Utc,
};

use super::{
    ApplicationMemberRow,
    ApplicationRow,
    CorrelationConn,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn upsert_application(&mut self, row: &ApplicationRow) -> DomainResult<i64> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_application_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_application_postgres.sql"),
        );
        let values = [
            BindValue::Text(&row.identity_kind),
            BindValue::Text(&row.identity_key),
            BindValue::Text(&row.name),
            BindValue::Bool(row.hidden),
            BindValue::OptTimestamp(row.archived_at),
            BindValue::Timestamp(row.created_at),
        ];
        self.insert_returning_id(sql, &values).await
    }

    pub async fn set_application_archived(
        &mut self, id: i64, archived_at: Option<DateTime<Utc>>,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE applications SET archived_at = {} WHERE id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let values = [BindValue::OptTimestamp(archived_at), BindValue::Int(id)];
        Ok(self.execute(&sql, &values).await? > 0)
    }

    pub async fn delete_application(&mut self, id: i64) -> DomainResult<bool> {
        let removed = self
            .delete_where("applications", &["id"], &[BindValue::Int(id)])
            .await?;
        Ok(removed > 0)
    }

    pub async fn get_application(&mut self, id: i64) -> DomainResult<Option<ApplicationRow>> {
        self.get(&["id"], &[BindValue::Int(id)]).await
    }

    pub async fn get_application_by_identity(
        &mut self, identity_kind: &str, identity_key: &str,
    ) -> DomainResult<Option<ApplicationRow>> {
        let values = [
            BindValue::Text(identity_kind),
            BindValue::Text(identity_key),
        ];
        self.get(&["identity_kind", "identity_key"], &values).await
    }

    pub async fn list_applications(&mut self) -> DomainResult<Vec<ApplicationRow>> {
        self.list(&[], &[]).await
    }

    pub async fn upsert_application_member(
        &mut self, row: &ApplicationMemberRow,
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_application_member_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_application_member_postgres.sql"),
        );
        let values = [
            BindValue::Int(row.application_id),
            BindValue::Text(&row.pipeline_id),
            BindValue::Text(&row.stage),
            BindValue::Text(&row.source),
            BindValue::OptInt(row.edge_id),
        ];
        self.execute(sql, &values).await?;
        Ok(())
    }

    pub async fn replace_application_members(
        &mut self, application_id: i64, rows: &[ApplicationMemberRow],
    ) -> DomainResult<()> {
        let mut scope = self.scope().await?;
        scope
            .delete_where(
                "application_members",
                &["application_id"],
                &[BindValue::Int(application_id)],
            )
            .await?;
        for row in rows {
            let row = ApplicationMemberRow {
                application_id,
                ..row.clone()
            };
            scope.upsert_application_member(&row).await?;
        }
        scope.finish().await
    }

    pub async fn delete_application_member(
        &mut self, application_id: i64, pipeline_id: &str,
    ) -> DomainResult<bool> {
        let values = [BindValue::Int(application_id), BindValue::Text(pipeline_id)];
        let removed = self
            .delete_where(
                "application_members",
                &["application_id", "pipeline_id"],
                &values,
            )
            .await?;
        Ok(removed > 0)
    }

    pub async fn list_application_members(&mut self) -> DomainResult<Vec<ApplicationMemberRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_application_members_by_application(
        &mut self, application_id: i64,
    ) -> DomainResult<Vec<ApplicationMemberRow>> {
        self.list(&["application_id"], &[BindValue::Int(application_id)])
            .await
    }

    pub async fn list_application_members_by_pipeline(
        &mut self, pipeline_id: &str,
    ) -> DomainResult<Vec<ApplicationMemberRow>> {
        self.list(&["pipeline_id"], &[BindValue::Text(pipeline_id)])
            .await
    }
}
