use chrono::Utc;

use super::{
    rows,
    MetricsRepository,
};
use crate::domain::{
    DomainResult,
    GlobalMetricsConfig,
    MetricsConfig,
    MetricsConfigExport,
};
use crate::infrastructure::database::bind::{
    execute_bound,
    fetch_rows,
    BindValue,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

const BATCH_SIZE: usize = 100;

impl MetricsRepository {
    pub async fn get_global_config(&self) -> DomainResult<GlobalMetricsConfig> {
        let sql = format!(
            "SELECT enabled, {}, updated_at FROM metrics_global_config WHERE id = 1",
            self.pool.bigint("default_retention_days")
        );
        let rows = fetch_rows(
            &self.pool,
            &sql,
            &[],
            rows::global_config_sqlite,
            rows::global_config_postgres,
        )
        .await?;

        Ok(rows.into_iter().next().unwrap_or_default())
    }

    pub async fn update_global_config(
        &self, enabled: bool, default_retention_days: i64,
    ) -> DomainResult<()> {
        let sql = format!(
            "UPDATE metrics_global_config SET enabled = {}, default_retention_days = {}, updated_at = {} WHERE id = 1",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.datetime_now()
        );
        let values = [
            BindValue::Bool(enabled),
            BindValue::Int(default_retention_days),
        ];

        execute_bound(&self.pool, &sql, &values).await?;
        Ok(())
    }

    fn pipeline_config_select(&self) -> String {
        format!(
            "SELECT pipeline_id, enabled, {}, created_at, updated_at FROM metrics_config",
            self.pool.bigint("retention_days")
        )
    }

    pub async fn get_pipeline_config(
        &self, pipeline_id: &str,
    ) -> DomainResult<Option<MetricsConfig>> {
        let sql = format!(
            "{} WHERE pipeline_id = {}",
            self.pipeline_config_select(),
            self.pool.placeholder(1)
        );
        let rows = fetch_rows(
            &self.pool,
            &sql,
            &[BindValue::Text(pipeline_id)],
            rows::metrics_config_sqlite,
            rows::metrics_config_postgres,
        )
        .await?;

        Ok(rows.into_iter().next())
    }

    pub async fn upsert_pipeline_config(
        &self, pipeline_id: &str, enabled: bool, retention_days: i64,
    ) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO metrics_config (pipeline_id, enabled, retention_days, created_at, updated_at)
             VALUES ({})
             ON CONFLICT(pipeline_id) DO UPDATE SET
                 enabled = excluded.enabled,
                 retention_days = excluded.retention_days,
                 updated_at = excluded.updated_at",
            self.pool.placeholders(5)
        );
        let now = Utc::now();
        let values = [
            BindValue::Text(pipeline_id),
            BindValue::Bool(enabled),
            BindValue::Int(retention_days),
            BindValue::Timestamp(now),
            BindValue::Timestamp(now),
        ];

        execute_bound(&self.pool, &sql, &values).await?;
        Ok(())
    }

    pub async fn export_config(&self) -> DomainResult<MetricsConfigExport> {
        let global_config = self.get_global_config().await?;
        let pipeline_configs = fetch_rows(
            &self.pool,
            &self.pipeline_config_select(),
            &[],
            rows::metrics_config_sqlite,
            rows::metrics_config_postgres,
        )
        .await?;

        Ok(MetricsConfigExport {
            global_config,
            pipeline_configs,
        })
    }

    pub async fn import_config(&self, export: &MetricsConfigExport) -> DomainResult<()> {
        let start = std::time::Instant::now();

        self.update_global_config(
            export.global_config.enabled,
            export.global_config.default_retention_days,
        )
        .await?;

        if export.pipeline_configs.is_empty() {
            return Ok(());
        }

        on_pool!(&self.pool, p => async {
            let mut tx = p.begin().await.map_err(db_err)?;
            for chunk in export.pipeline_configs.chunks(BATCH_SIZE) {
                let mut query = sqlx::query(sqlx::AssertSqlSafe(self.upsert_configs_sql(chunk.len())));
                for config in chunk {
                    query = query
                        .bind(&config.pipeline_id)
                        .bind(config.enabled)
                        .bind(config.retention_days);
                }
                query.execute(&mut *tx).await.map_err(db_err)?;
            }
            tx.commit().await.map_err(db_err)
        }
        .await)?;

        tracing::debug!(
            configs_count = export.pipeline_configs.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "Imported metrics config (batch upsert)"
        );

        Ok(())
    }

    fn upsert_configs_sql(&self, rows: usize) -> String {
        let now = self.pool.datetime_now();
        let values = (0..rows)
            .map(|row| {
                let at = |column: usize| self.pool.placeholder(row * 3 + column);
                format!("({}, {}, {}, {now}, {now})", at(1), at(2), at(3))
            })
            .collect::<Vec<_>>()
            .join(", ");
        let columns =
            "metrics_config (pipeline_id, enabled, retention_days, created_at, updated_at)";

        match self.pool {
            DatabasePool::Sqlite(_) => format!("INSERT OR REPLACE INTO {columns} VALUES {values}"),
            DatabasePool::Postgres(_) => format!(
                "INSERT INTO {columns} VALUES {values}
                 ON CONFLICT (pipeline_id) DO UPDATE SET enabled = EXCLUDED.enabled, retention_days = EXCLUDED.retention_days, updated_at = EXCLUDED.updated_at"
            ),
        }
    }
}
