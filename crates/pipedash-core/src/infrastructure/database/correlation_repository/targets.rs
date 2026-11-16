use super::{
    ApplicationTargetRow,
    CorrelationConn,
    TargetDirectionRow,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn replace_application_targets(
        &mut self, application_id: i64, rows: &[ApplicationTargetRow],
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_application_target_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_application_target_postgres.sql"),
        );
        let mut scope = self.scope().await?;
        scope
            .delete_where(
                "application_targets",
                &["application_id"],
                &[BindValue::Int(application_id)],
            )
            .await?;
        for row in rows {
            let values = [
                BindValue::Int(application_id),
                BindValue::Text(&row.target_key),
                BindValue::Text(&row.name),
                BindValue::Text(&row.group_key),
                BindValue::Text(&row.lifetime),
            ];
            scope.execute(sql, &values).await?;
        }
        scope.finish().await
    }

    pub async fn list_application_targets(&mut self) -> DomainResult<Vec<ApplicationTargetRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_application_targets_by_application(
        &mut self, application_id: i64,
    ) -> DomainResult<Vec<ApplicationTargetRow>> {
        self.list(&["application_id"], &[BindValue::Int(application_id)])
            .await
    }

    pub async fn replace_target_directions(
        &mut self, application_id: i64, rows: &[TargetDirectionRow],
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_target_direction_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_target_direction_postgres.sql"),
        );
        let mut scope = self.scope().await?;
        scope
            .delete_where(
                "target_directions",
                &["application_id"],
                &[BindValue::Int(application_id)],
            )
            .await?;
        for row in rows {
            let values = [
                BindValue::Int(application_id),
                BindValue::Text(&row.from_group),
                BindValue::Text(&row.to_group),
                BindValue::Text(&row.source),
            ];
            scope.execute(sql, &values).await?;
        }
        scope.finish().await
    }

    pub async fn list_target_directions(&mut self) -> DomainResult<Vec<TargetDirectionRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_target_directions_by_application(
        &mut self, application_id: i64,
    ) -> DomainResult<Vec<TargetDirectionRow>> {
        self.list(&["application_id"], &[BindValue::Int(application_id)])
            .await
    }
}
