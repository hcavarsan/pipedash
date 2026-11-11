mod members;
mod pins;
mod rows;
#[cfg(test)]
mod tests;
mod transfer;
mod views;

use rows::{
    parse_pin_kind,
    stored_view,
    MemberRow,
    PinRow,
    ViewRow,
};

use super::pool::db_err;
use super::DatabasePool;
use crate::domain::{
    DomainResult,
    PinKind,
    ViewDisplay,
    ViewKind,
    ViewRules,
};

#[derive(Debug, Clone, PartialEq)]
pub struct StoredView {
    pub id: i64,
    pub kind: ViewKind,
    pub name: String,
    pub parent_id: Option<i64>,
    pub position: i64,
    pub rules: ViewRules,
    pub display: ViewDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMember {
    pub view_id: i64,
    pub provider_id: i64,
    pub pipeline_id: String,
    pub position: i64,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPin {
    pub kind: PinKind,
    pub reference: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewExport {
    pub views: Vec<StoredView>,
    pub members: Vec<StoredMember>,
    pub pins: Vec<StoredPin>,
}

pub struct PipelineViewRepository {
    pool: DatabasePool,
}

impl PipelineViewRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub async fn list_views(&self) -> DomainResult<Vec<StoredView>> {
        let sql = "SELECT id, kind, name, position, rules_json, display_json, parent_id FROM pipeline_views ORDER BY position, id";
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, ViewRow>(sql)
            .fetch_all(p)
            .await)
        .map_err(db_err)?;
        Ok(rows.into_iter().filter_map(stored_view).collect())
    }

    pub async fn list_members(&self) -> DomainResult<Vec<StoredMember>> {
        let sql = "SELECT view_id, provider_id, pipeline_id, position, excluded FROM pipeline_view_members ORDER BY view_id, position, pipeline_id";
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, MemberRow>(sql)
            .fetch_all(p)
            .await)
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(
                |(view_id, provider_id, pipeline_id, position, excluded)| StoredMember {
                    view_id,
                    provider_id,
                    pipeline_id,
                    position,
                    excluded,
                },
            )
            .collect())
    }

    pub async fn list_pins(&self) -> DomainResult<Vec<StoredPin>> {
        let sql = "SELECT kind, ref FROM pipeline_view_pins ORDER BY position, id";
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, PinRow>(sql)
            .fetch_all(p)
            .await)
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .filter_map(|(kind, reference)| {
                Some(StoredPin {
                    kind: parse_pin_kind(&kind)?,
                    reference,
                })
            })
            .collect())
    }
}
