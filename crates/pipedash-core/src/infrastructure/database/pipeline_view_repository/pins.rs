use super::PipelineViewRepository;
use crate::domain::{
    DomainResult,
    PinKind,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

impl PipelineViewRepository {
    pub(super) fn pin_insert_sql(&self) -> String {
        format!(
            "INSERT INTO pipeline_view_pins (kind, ref, position)
             VALUES ({}, {}, (SELECT COALESCE(MAX(position), -1) + 1 FROM pipeline_view_pins))
             ON CONFLICT (kind, ref) DO NOTHING",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
        )
    }

    async fn execute_pin(&self, sql: String, kind: PinKind, reference: &str) -> DomainResult<()> {
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(kind.as_str())
            .bind(reference)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_err)
    }

    pub async fn insert_pin(&self, kind: PinKind, reference: &str) -> DomainResult<()> {
        self.execute_pin(self.pin_insert_sql(), kind, reference)
            .await
    }

    pub async fn delete_pin(&self, kind: PinKind, reference: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM pipeline_view_pins WHERE kind = {} AND ref = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
        );
        self.execute_pin(sql, kind, reference).await
    }
}
