use std::collections::HashMap;

use serde_json::Value;

use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::retry::retry_on_busy;
use crate::infrastructure::database::DatabasePool;

const POSTGRES_UPSERT: &str = "INSERT INTO run_inputs (pipeline_id, run_number, inputs_json) VALUES ($1, $2, $3) ON CONFLICT (pipeline_id, run_number) DO UPDATE SET inputs_json = EXCLUDED.inputs_json";
const SQLITE_UPSERT: &str =
    "INSERT OR REPLACE INTO run_inputs (pipeline_id, run_number, inputs_json) VALUES (?, ?, ?)";

fn parse_inputs(json: &str) -> DomainResult<Value> {
    serde_json::from_str(json)
        .map_err(|error| DomainError::DatabaseError(format!("Invalid run inputs: {error}")))
}

impl Repository {
    pub async fn record_run_inputs(
        &self, pipeline_id: &str, run_number: i64, inputs: &Value,
    ) -> DomainResult<()> {
        let sql = self.cache_pool.pick(SQLITE_UPSERT, POSTGRES_UPSERT);
        let inputs_json = inputs.to_string();
        retry_on_busy(|| async {
            on_pool!(&self.cache_pool, p => sqlx::query(sql)
                .bind(pipeline_id)
                .bind(run_number)
                .bind(&inputs_json)
                .execute(p)
                .await
                .map(|_| ())
                .map_err(db_err))
        })
        .await
    }

    pub async fn get_run_inputs(&self, pipeline_id: &str) -> DomainResult<HashMap<i64, Value>> {
        let sql = format!(
            "SELECT run_number, inputs_json FROM run_inputs WHERE pipeline_id = {}",
            self.cache_pool.placeholder(1)
        );
        let rows: Vec<(i64, String)> = on_pool!(&self.cache_pool, p => sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(pipeline_id)
            .fetch_all(p)
            .await
            .map_err(db_err))?;

        rows.into_iter()
            .map(|(run_number, json)| Ok((run_number, parse_inputs(&json)?)))
            .collect()
    }

    pub(crate) async fn list_run_inputs(&self) -> DomainResult<Vec<(String, i64, Value)>> {
        let rows: Vec<(String, i64, String)> = on_pool!(&self.cache_pool, p => sqlx::query_as(
            "SELECT pipeline_id, run_number, inputs_json FROM run_inputs"
        )
            .fetch_all(p)
            .await
            .map_err(db_err))?;

        rows.into_iter()
            .map(|(pipeline_id, run_number, json)| {
                Ok((pipeline_id, run_number, parse_inputs(&json)?))
            })
            .collect()
    }
}
