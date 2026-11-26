use super::{
    CorrelationConn,
    CorrelationEdgeRow,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn upsert_edge(&mut self, row: &CorrelationEdgeRow) -> DomainResult<i64> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_edge_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_edge_postgres.sql"),
        );
        let values = [
            BindValue::Text(&row.from_kind),
            BindValue::Text(&row.from_id),
            BindValue::Text(&row.to_kind),
            BindValue::Text(&row.to_id),
            BindValue::Text(&row.relation),
            BindValue::Text(&row.confidence),
            BindValue::Text(&row.rule),
            BindValue::Text(&row.evidence_json),
            BindValue::Timestamp(row.first_seen),
            BindValue::Timestamp(row.last_seen),
        ];
        self.insert_returning_id(sql, &values).await
    }

    pub async fn delete_edge(&mut self, id: i64) -> DomainResult<bool> {
        let removed = self
            .delete_where("correlation_edges", &["id"], &[BindValue::Int(id)])
            .await?;
        Ok(removed > 0)
    }

    pub async fn delete_edges_from(&mut self, from_kind: &str, from_id: &str) -> DomainResult<u64> {
        let values = [BindValue::Text(from_kind), BindValue::Text(from_id)];
        self.delete_where("correlation_edges", &["from_kind", "from_id"], &values)
            .await
    }

    pub async fn delete_edges_to(&mut self, to_kind: &str, to_id: &str) -> DomainResult<u64> {
        let values = [BindValue::Text(to_kind), BindValue::Text(to_id)];
        self.delete_where("correlation_edges", &["to_kind", "to_id"], &values)
            .await
    }

    pub async fn get_edge(&mut self, id: i64) -> DomainResult<Option<CorrelationEdgeRow>> {
        self.get(&["id"], &[BindValue::Int(id)]).await
    }

    pub async fn list_edges(&mut self) -> DomainResult<Vec<CorrelationEdgeRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_edges_from(
        &mut self, from_kind: &str, from_id: &str,
    ) -> DomainResult<Vec<CorrelationEdgeRow>> {
        let values = [BindValue::Text(from_kind), BindValue::Text(from_id)];
        self.list(&["from_kind", "from_id"], &values).await
    }

    pub async fn list_edges_to(
        &mut self, to_kind: &str, to_id: &str,
    ) -> DomainResult<Vec<CorrelationEdgeRow>> {
        let values = [BindValue::Text(to_kind), BindValue::Text(to_id)];
        self.list(&["to_kind", "to_id"], &values).await
    }

    pub async fn list_edges_by_rule(
        &mut self, rule: &str,
    ) -> DomainResult<Vec<CorrelationEdgeRow>> {
        self.list(&["rule"], &[BindValue::Text(rule)]).await
    }
}
