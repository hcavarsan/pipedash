use std::collections::HashMap;

use chrono::Utc;

use super::pipeline_row::{
    resource_state_json,
    PIPELINE_SELECT,
};
use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::retry::retry_on_busy;
use crate::infrastructure::database::DatabasePool;

macro_rules! bind_pipeline_fields {
    ($query:expr, $pipeline:expr, $now:expr, $metadata:expr, $state:expr) => {
        $query
            .bind(&$pipeline.name)
            .bind($pipeline.status.as_str())
            .bind(&$pipeline.repository)
            .bind(&$pipeline.branch)
            .bind(&$pipeline.workflow_file)
            .bind($pipeline.last_run)
            .bind($now)
            .bind($metadata)
            .bind(&$pipeline.provider_type)
            .bind($state)
    };
}

struct Reconciliation<'a> {
    updates: Vec<&'a Pipeline>,
    inserts: Vec<&'a Pipeline>,
    deletes: Vec<&'a str>,
}

fn pipeline_changed(old: &Pipeline, new: &Pipeline, provider_id: i64) -> bool {
    old.status != new.status
        || old.last_run != new.last_run
        || old.name != new.name
        || old.provider_id != provider_id
        || old.provider_type != new.provider_type
        || old.repository != new.repository
        || old.branch != new.branch
        || old.workflow_file != new.workflow_file
        || old.metadata != new.metadata
        || old.resource_state != new.resource_state
}

fn reconciliation<'a>(
    existing: &'a HashMap<String, Pipeline>, pipelines: &'a [Pipeline], provider_id: i64,
) -> Reconciliation<'a> {
    let mut plan = Reconciliation {
        updates: Vec::new(),
        inserts: Vec::new(),
        deletes: Vec::new(),
    };
    for pipeline in pipelines {
        match existing.get(&pipeline.id) {
            None => plan.inserts.push(pipeline),
            Some(old) if pipeline_changed(old, pipeline, provider_id) => {
                plan.updates.push(pipeline)
            }
            Some(_) => {}
        }
    }
    let kept: std::collections::HashSet<&str> = pipelines.iter().map(|p| p.id.as_str()).collect();
    plan.deletes = existing
        .keys()
        .map(String::as_str)
        .filter(|id| !kept.contains(id))
        .collect();
    plan
}

fn serialized(pipeline: &Pipeline) -> DomainResult<(String, Option<String>)> {
    Ok((
        serde_json::to_string(&pipeline.metadata).map_err(db_err)?,
        resource_state_json(pipeline)?,
    ))
}

impl Repository {
    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        let sql = match provider_id {
            Some(_) => format!(
                "{PIPELINE_SELECT} WHERE provider_id = {} ORDER BY last_updated DESC",
                self.cache_pool.placeholder(1)
            ),
            None => format!("{PIPELINE_SELECT} ORDER BY last_updated DESC"),
        };
        on_pool!(&self.cache_pool, p => {
            let query = sqlx::query(sqlx::AssertSqlSafe(sql));
            let query = match provider_id {
                Some(id) => query.bind(id),
                None => query,
            };
            let rows = query.fetch_all(p).await.map_err(db_err)?;
            rows.iter().map(|row| self.pipeline_from_row(row)).collect()
        })
    }

    pub async fn get_cached_pipeline(&self, pipeline_id: &str) -> DomainResult<Pipeline> {
        let sql = format!(
            "{PIPELINE_SELECT} WHERE id = {}",
            self.cache_pool.placeholder(1)
        );
        let pipeline = on_pool!(&self.cache_pool, p => {
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(pipeline_id)
                .fetch_optional(p)
                .await
                .map_err(db_err)?;
            row.as_ref()
                .map(|row| self.pipeline_from_row(row))
                .transpose()
        })?;
        pipeline.ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))
    }

    pub async fn update_pipelines_cache(
        &self, provider_id: i64, new_pipelines: &[Pipeline],
    ) -> DomainResult<()> {
        let pool = &self.cache_pool;
        let select_sql = pool.pick(
            include_str!("../../../../sql/cache/select_pipelines_sqlite.sql"),
            include_str!("../../../../sql/cache/select_pipelines_postgres.sql"),
        );
        let update_sql = pool.pick(
            include_str!("../../../../sql/cache/update_pipeline_sqlite.sql"),
            include_str!("../../../../sql/cache/update_pipeline_postgres.sql"),
        );
        let insert_sql = pool.pick(
            include_str!("../../../../sql/cache/insert_pipeline_sqlite.sql"),
            include_str!("../../../../sql/cache/insert_pipeline_postgres.sql"),
        );
        let delete_sql = format!(
            "DELETE FROM pipelines_cache WHERE id = {}",
            pool.placeholder(1)
        );
        retry_on_busy(|| async {
            on_pool!(pool, p => {
                let mut tx = p.begin().await.map_err(db_err)?;
                let rows = sqlx::query(select_sql)
                    .bind(provider_id)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(db_err)?;
                let existing = rows
                    .iter()
                    .map(|row| self.pipeline_from_row(row).map(|p| (p.id.clone(), p)))
                    .collect::<DomainResult<HashMap<_, _>>>()?;
                let plan = reconciliation(&existing, new_pipelines, provider_id);
                let now = Utc::now();

                for pipeline in plan.updates {
                    let (metadata, state) = serialized(pipeline)?;
                    let query = sqlx::query(update_sql).bind(provider_id);
                    bind_pipeline_fields!(query, pipeline, now, metadata, state)
                        .bind(&pipeline.id)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                }
                for pipeline in plan.inserts {
                    let (metadata, state) = serialized(pipeline)?;
                    let query = sqlx::query(insert_sql).bind(&pipeline.id).bind(provider_id);
                    bind_pipeline_fields!(query, pipeline, now, metadata, state)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                }
                for id in plan.deletes {
                    sqlx::query(sqlx::AssertSqlSafe(delete_sql.as_str()))
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                }

                tx.commit().await.map_err(db_err)
            })
        })
        .await
    }
}
