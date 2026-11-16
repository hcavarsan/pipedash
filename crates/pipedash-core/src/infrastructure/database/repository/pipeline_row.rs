use chrono::{
    DateTime,
    Utc,
};
use sqlx::Row as SqlxRow;

use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineStatus,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

pub(super) const PIPELINE_SELECT: &str = "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type, metadata_json, resource_state_json FROM pipelines_cache";

fn parse_status(status_str: &str) -> DomainResult<PipelineStatus> {
    let json = if status_str.trim().is_empty() {
        return Ok(PipelineStatus::Pending);
    } else if status_str.starts_with('"') {
        status_str.to_string()
    } else {
        format!("\"{}\"", status_str)
    };
    serde_json::from_str(&json).map_err(|e| {
        DomainError::DatabaseError(format!("Failed to parse status '{}': {}", status_str, e))
    })
}

impl Repository {
    pub(super) fn pipeline_from_row<'r, R>(&self, row: &'r R) -> DomainResult<Pipeline>
    where
        R: SqlxRow,
        usize: sqlx::ColumnIndex<R>,
        String: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        i64: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        Option<String>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        DateTime<Utc>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        Option<DateTime<Utc>>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    {
        let id: String = row.try_get(0).map_err(db_err)?;
        let provider_id: i64 = row.try_get(1).map_err(db_err)?;
        let name: String = row.try_get(2).map_err(db_err)?;
        let status_str: String = row.try_get(3).map_err(db_err)?;
        let repository: String = row.try_get(4).map_err(db_err)?;
        let branch: Option<String> = row.try_get(5).map_err(db_err)?;
        let workflow_file: Option<String> = row.try_get(6).map_err(db_err)?;

        let status = parse_status(&status_str)?;

        let (last_run, last_updated) = match &self.cache_pool {
            DatabasePool::Sqlite(_) => {
                let last_run_str: Option<String> = row.try_get(7).map_err(db_err)?;
                let last_updated_str: String = row.try_get(8).map_err(db_err)?;

                let last_run = last_run_str
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc));
                let last_updated = DateTime::parse_from_rfc3339(&last_updated_str)
                    .map_err(db_err)?
                    .with_timezone(&Utc);

                (last_run, last_updated)
            }
            DatabasePool::Postgres(_) => {
                let last_run: Option<DateTime<Utc>> = row.try_get(7).map_err(db_err)?;
                let last_updated: DateTime<Utc> = row.try_get(8).map_err(db_err)?;

                (last_run, last_updated)
            }
        };

        let provider_type: String = row.try_get(9).map_err(db_err)?;
        let metadata_json: String = row.try_get(10).map_err(db_err)?;
        let metadata = serde_json::from_str(&metadata_json).map_err(|error| {
            DomainError::DatabaseError(format!("Invalid pipeline metadata: {error}"))
        })?;
        let resource_state = row
            .try_get::<Option<String>, _>(11)
            .map_err(db_err)?
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|error| {
                DomainError::DatabaseError(format!("Invalid pipeline resource state: {error}"))
            })?;

        Ok(Pipeline {
            id,
            provider_id,
            provider_type,
            name,
            status,
            last_run,
            last_updated,
            repository,
            branch,
            workflow_file,
            resource_state,
            metadata,
            overdue_run: None,
            typical_duration_seconds: None,
            actions: Default::default(),
            correlation: Default::default(),
        })
    }
}

pub(super) fn resource_state_json(pipeline: &Pipeline) -> DomainResult<Option<String>> {
    pipeline
        .resource_state
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(db_err)
}
