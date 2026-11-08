use std::collections::HashMap;
use std::time::Instant;

use chrono::{
    DateTime,
    Duration,
    Utc,
};

use super::MetricsRepository;
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::{
    execute_bound,
    BindValue,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

impl MetricsRepository {
    async fn delete_older_than(
        &self, pipeline_ids: &[String], cutoff: DateTime<Utc>,
    ) -> DomainResult<usize> {
        let cutoff_placeholder = self.pool.placeholder(pipeline_ids.len() + 1);
        let comparison = match self.pool {
            DatabasePool::Sqlite(_) => {
                format!("datetime(timestamp) < datetime({cutoff_placeholder})")
            }
            DatabasePool::Postgres(_) => format!("timestamp < {cutoff_placeholder}"),
        };
        let sql = format!(
            "DELETE FROM pipeline_metrics WHERE pipeline_id IN ({}) AND {comparison}",
            self.pool.placeholders(pipeline_ids.len())
        );
        let values: Vec<BindValue> = pipeline_ids
            .iter()
            .map(|id| BindValue::Text(id.as_str()))
            .chain([BindValue::Timestamp(cutoff)])
            .collect();

        Ok(execute_bound(&self.pool, &sql, &values).await? as usize)
    }

    async fn pipelines_by_retention(
        &self, default_days: i64,
    ) -> DomainResult<HashMap<i64, Vec<String>>> {
        let sql = format!(
            "SELECT pipeline_id, {} FROM metrics_config",
            self.pool.bigint("retention_days")
        );
        let configs: HashMap<String, i64> =
            on_pool!(&self.pool, p => sqlx::query_as::<_, (String, i64)>(sqlx::AssertSqlSafe(sql))
                .fetch_all(p)
                .await)
            .map_err(db_err)?
            .into_iter()
            .collect();

        let mut grouped: HashMap<i64, Vec<String>> = HashMap::new();
        for pipeline_id in self.list_pipeline_ids().await? {
            let days = configs.get(&pipeline_id).copied().unwrap_or(default_days);
            grouped.entry(days).or_default().push(pipeline_id);
        }

        Ok(grouped)
    }

    pub async fn delete_old_metrics(&self, pipeline_id: Option<&str>) -> DomainResult<usize> {
        let start = Instant::now();
        let global_config = self.get_global_config().await?;
        let mut total_deleted = 0;

        if let Some(pid) = pipeline_id {
            let retention_days = self
                .get_pipeline_config(pid)
                .await?
                .map_or(global_config.default_retention_days, |c| c.retention_days);
            let cutoff = Utc::now() - Duration::days(retention_days);

            total_deleted += self.delete_older_than(&[pid.to_string()], cutoff).await?;
        } else {
            let grouped = self
                .pipelines_by_retention(global_config.default_retention_days)
                .await?;
            if grouped.is_empty() {
                return Ok(0);
            }

            for (retention_days, pipeline_ids) in grouped {
                let cutoff = Utc::now() - Duration::days(retention_days);
                total_deleted += self.delete_older_than(&pipeline_ids, cutoff).await?;
            }
        }

        let now = Utc::now();
        let sql = format!(
            "UPDATE metrics_storage_info SET last_cleanup_at = {}, updated_at = {} WHERE id = 1",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let values = [BindValue::Timestamp(now), BindValue::Timestamp(now)];
        execute_bound(&self.pool, &sql, &values).await?;

        tracing::debug!(
            pipeline_id = pipeline_id,
            total_deleted = total_deleted,
            elapsed_ms = start.elapsed().as_millis(),
            "Deleted old metrics"
        );

        Ok(total_deleted)
    }

    pub async fn flush_metrics(
        &self, pipeline_id: Option<&str>, skip_vacuum: bool,
    ) -> DomainResult<usize> {
        let deleted_count = (match pipeline_id {
            Some(pid) => {
                let sql = format!(
                    "DELETE FROM pipeline_metrics WHERE pipeline_id = {}",
                    self.pool.placeholder(1)
                );
                execute_bound(&self.pool, &sql, &[BindValue::Text(pid)]).await?
            }
            None => execute_bound(&self.pool, "DELETE FROM pipeline_metrics", &[]).await?,
        }) as usize;

        if let DatabasePool::Sqlite(p) = &self.pool {
            if deleted_count > 0 && !skip_vacuum {
                tracing::debug!(
                    deleted = deleted_count,
                    "Running VACUUM after flushing metrics"
                );
                sqlx::query("VACUUM").execute(p).await.map_err(db_err)?;
                tracing::debug!("VACUUM complete");
            }
        }

        Ok(deleted_count)
    }
}
