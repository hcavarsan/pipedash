use chrono::{
    DateTime,
    Utc,
};
use sqlx::Row as SqlxRow;

use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::{
    pool::db_err,
    retry::retry_on_busy,
    DatabasePool,
};

pub(crate) const FETCH_STATUS_SUCCESS: &str = "success";
pub(crate) const FETCH_STATUS_ERROR: &str = "error";
pub(crate) const FETCH_STATUS_NEVER: &str = "never";

pub struct ProviderFetchStatus {
    pub status: String,
    pub error: Option<String>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub succeeded_at: Option<DateTime<Utc>>,
}

impl Repository {
    pub async fn update_provider_fetch_status(
        &self, provider_id: i64, success: bool, error: Option<String>,
    ) -> DomainResult<bool> {
        let new_status = if success {
            FETCH_STATUS_SUCCESS
        } else {
            FETCH_STATUS_ERROR
        };
        let select_sql = format!(
            "SELECT last_fetch_status, last_fetch_error FROM providers WHERE id = {}",
            self.cache_pool.placeholder(1)
        );

        retry_on_busy(|| async {
            let now = Utc::now();

            let status_changed = on_pool!(&self.cache_pool, p => {
                let current = sqlx::query(sqlx::AssertSqlSafe(select_sql.clone()))
                    .bind(provider_id)
                    .fetch_optional(p)
                    .await
                    .map_err(db_err)?;
                Ok::<bool, DomainError>(current.is_none_or(|row| {
                    let current_status: String =
                        row.try_get("last_fetch_status").unwrap_or_default();
                    let current_error: Option<String> =
                        row.try_get("last_fetch_error").ok().flatten();
                    current_status != new_status || current_error != error
                }))
            })?;

            match &self.cache_pool {
                DatabasePool::Sqlite(p) => {
                    let update_sql = "UPDATE providers SET last_fetch_at = ?, last_fetch_status = ?, last_fetch_error = ?, last_success_at = COALESCE(?, last_success_at) WHERE id = ?";
                    sqlx::query(update_sql)
                        .bind(now.to_rfc3339())
                        .bind(new_status)
                        .bind(error.as_ref())
                        .bind(success.then(|| now.to_rfc3339()))
                        .bind(provider_id)
                        .execute(p)
                        .await
                        .map_err(db_err)?;
                }
                DatabasePool::Postgres(p) => {
                    let update_sql = "UPDATE providers SET last_fetch_at = $1, last_fetch_status = $2, last_fetch_error = $3, last_success_at = COALESCE($4, last_success_at) WHERE id = $5";
                    sqlx::query(update_sql)
                        .bind(now)
                        .bind(new_status)
                        .bind(error.as_ref())
                        .bind(success.then_some(now))
                        .bind(provider_id)
                        .execute(p)
                        .await
                        .map_err(db_err)?;
                }
            }

            Ok(status_changed)
        })
        .await
    }

    pub async fn get_provider_fetch_status(
        &self, provider_id: i64,
    ) -> DomainResult<ProviderFetchStatus> {
        let sql = format!(
            "SELECT last_fetch_status, last_fetch_error, last_fetch_at, last_success_at FROM providers WHERE id = {}",
            self.cache_pool.placeholder(1)
        );
        let not_found = |_| DomainError::ProviderNotFound(provider_id.to_string());
        let (row_status, error, fetched_at, succeeded_at) = match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(provider_id)
                    .fetch_one(p)
                    .await
                    .map_err(not_found)?;
                let time = |index: usize| {
                    row.try_get::<Option<String>, _>(index)
                        .ok()
                        .flatten()
                        .and_then(|at| DateTime::parse_from_rfc3339(&at).ok())
                        .map(|at| at.with_timezone(&Utc))
                };

                (
                    row.try_get::<String, _>(0).ok(),
                    row.try_get(1).ok().flatten(),
                    time(2),
                    time(3),
                )
            }
            DatabasePool::Postgres(p) => {
                let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(provider_id)
                    .fetch_one(p)
                    .await
                    .map_err(not_found)?;

                (
                    row.try_get::<String, _>(0).ok(),
                    row.try_get(1).ok().flatten(),
                    row.try_get(2).ok().flatten(),
                    row.try_get(3).ok().flatten(),
                )
            }
        };

        Ok(ProviderFetchStatus {
            status: row_status.unwrap_or_else(|| FETCH_STATUS_NEVER.to_string()),
            error,
            fetched_at,
            succeeded_at,
        })
    }

    pub async fn reset_provider_fetch_status(&self, provider_id: i64) -> DomainResult<()> {
        let sql = format!(
            "UPDATE providers SET last_fetch_at = NULL, last_success_at = NULL, last_fetch_status = {}, last_fetch_error = NULL WHERE id = {}",
            self.cache_pool.placeholder(1),
            self.cache_pool.placeholder(2)
        );

        retry_on_busy(|| async {
            on_pool!(&self.cache_pool, p => sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                .bind(FETCH_STATUS_NEVER)
                .bind(provider_id)
                .execute(p)
                .await
                .map(|_| ())
                .map_err(db_err))?;

            Ok(())
        })
        .await
    }
}
