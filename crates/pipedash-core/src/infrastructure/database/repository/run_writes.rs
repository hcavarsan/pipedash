use std::time::Instant;

use super::Repository;
use crate::domain::{
    DomainResult,
    PipelineRun,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::retry::retry_on_busy;
use crate::infrastructure::database::DatabasePool;
use crate::infrastructure::deduplication::hash_pipeline_run;

const BATCH_SIZE: usize = 100;
const PRESENTED_FIELDS: [&str; 3] = ["actions", "display", "pace"];
const UPSERT_COLUMNS: &str =
    "run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash, started_at_ms)";
const POSTGRES_CONFLICT: &str = " ON CONFLICT (pipeline_id, run_number) DO UPDATE SET run_data = EXCLUDED.run_data, fetched_at = EXCLUDED.fetched_at, run_hash = EXCLUDED.run_hash, started_at_ms = EXCLUDED.started_at_ms";

type RunRow = (i64, String, String, i64);

fn run_cache_row(run: &PipelineRun) -> DomainResult<RunRow> {
    let mut value = serde_json::to_value(run).map_err(db_err)?;
    if let Some(fields) = value.as_object_mut() {
        for presented in PRESENTED_FIELDS {
            fields.remove(presented);
        }
    }
    let run_data = value.to_string();

    Ok((
        run.run_number,
        run_data,
        hash_pipeline_run(run),
        run.started_at.timestamp_millis(),
    ))
}

fn upsert_sql(pool: &DatabasePool, rows: usize) -> String {
    let values = (0..rows)
        .map(|row| {
            let at = |column: usize| pool.placeholder(row * 5 + column);
            format!(
                "({}, {}, {}, {}, {}, {})",
                at(1),
                at(2),
                at(3),
                pool.datetime_now(),
                at(4),
                at(5)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    if pool.is_postgres() {
        format!("INSERT INTO {UPSERT_COLUMNS} VALUES {values}{POSTGRES_CONFLICT}")
    } else {
        format!("INSERT OR REPLACE INTO {UPSERT_COLUMNS} VALUES {values}")
    }
}

fn delete_sql(pool: &DatabasePool, run_numbers: usize) -> String {
    let placeholders = (0..run_numbers)
        .map(|index| pool.placeholder(index + 2))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "DELETE FROM run_history_cache WHERE pipeline_id = {} AND run_number IN ({placeholders})",
        pool.placeholder(1)
    )
}

async fn write_runs(
    pool: &DatabasePool, pipeline_id: &str, upserts: &[&PipelineRun], deleted: &[i64],
) -> DomainResult<()> {
    let rows = upserts
        .iter()
        .copied()
        .map(run_cache_row)
        .collect::<DomainResult<Vec<_>>>()?;

    on_pool!(pool, p => {
        let mut tx = p.begin().await.map_err(db_err)?;

        for chunk in rows.chunks(BATCH_SIZE) {
            let mut query = sqlx::query(sqlx::AssertSqlSafe(upsert_sql(pool, chunk.len())));
            for (run_number, run_data, run_hash, started_at_ms) in chunk {
                query = query
                    .bind(pipeline_id)
                    .bind(run_number)
                    .bind(run_data)
                    .bind(run_hash)
                    .bind(started_at_ms);
            }
            query.execute(&mut *tx).await.map_err(db_err)?;
        }

        for chunk in deleted.chunks(BATCH_SIZE) {
            let mut query = sqlx::query(sqlx::AssertSqlSafe(delete_sql(pool, chunk.len())))
                .bind(pipeline_id);
            for run_number in chunk {
                query = query.bind(run_number);
            }
            query.execute(&mut *tx).await.map_err(db_err)?;
        }

        tx.commit().await.map_err(db_err)
    })
}

impl Repository {
    pub async fn cache_run_history(
        &self, pipeline_id: &str, runs: &[PipelineRun],
    ) -> DomainResult<()> {
        if runs.is_empty() {
            return Ok(());
        }

        let start = Instant::now();
        let upserts: Vec<&PipelineRun> = runs.iter().collect();

        retry_on_busy(|| write_runs(&self.cache_pool, pipeline_id, &upserts, &[])).await?;

        tracing::debug!(
            pipeline_id = pipeline_id,
            runs_count = runs.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "Cached run history (batch insert)"
        );

        Ok(())
    }

    pub async fn merge_run_cache(
        &self, pipeline_id: &str, new_runs: Vec<PipelineRun>, changed_runs: Vec<PipelineRun>,
        deleted_run_numbers: Vec<i64>,
    ) -> DomainResult<()> {
        let upserts: Vec<&PipelineRun> = new_runs.iter().chain(changed_runs.iter()).collect();
        if upserts.is_empty() && deleted_run_numbers.is_empty() {
            return Ok(());
        }

        let start = Instant::now();
        retry_on_busy(|| {
            write_runs(
                &self.cache_pool,
                pipeline_id,
                &upserts,
                &deleted_run_numbers,
            )
        })
        .await?;

        tracing::debug!(
            pipeline_id = pipeline_id,
            new_runs = new_runs.len(),
            changed_runs = changed_runs.len(),
            deleted_runs = deleted_run_numbers.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "Merged run cache (batch operations)"
        );

        Ok(())
    }
}
