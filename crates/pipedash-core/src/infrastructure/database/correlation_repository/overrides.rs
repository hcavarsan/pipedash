use super::{
    ApplicationOverrideRow,
    CorrelationConn,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn insert_override(&mut self, row: &ApplicationOverrideRow) -> DomainResult<i64> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_override_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_override_postgres.sql"),
        );
        let values = [
            BindValue::OptInt(row.application_id),
            BindValue::Text(&row.kind),
            BindValue::OptText(row.subject.as_deref()),
            BindValue::OptText(row.target.as_deref()),
            BindValue::Text(&row.value_json),
            BindValue::OptText(row.author.as_deref()),
            BindValue::Timestamp(row.created_at),
        ];
        self.insert_returning_id(sql, &values).await
    }

    pub async fn set_override_application(
        &mut self, id: i64, application_id: Option<i64>,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE application_overrides SET application_id = {} WHERE id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let values = [BindValue::OptInt(application_id), BindValue::Int(id)];
        Ok(self.execute(&sql, &values).await? > 0)
    }

    pub async fn delete_override(&mut self, id: i64) -> DomainResult<bool> {
        let removed = self
            .delete_where("application_overrides", &["id"], &[BindValue::Int(id)])
            .await?;
        Ok(removed > 0)
    }

    pub async fn get_override(&mut self, id: i64) -> DomainResult<Option<ApplicationOverrideRow>> {
        self.get(&["id"], &[BindValue::Int(id)]).await
    }

    pub async fn list_overrides(&mut self) -> DomainResult<Vec<ApplicationOverrideRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_overrides_by_application(
        &mut self, application_id: i64,
    ) -> DomainResult<Vec<ApplicationOverrideRow>> {
        self.list(&["application_id"], &[BindValue::Int(application_id)])
            .await
    }
}
