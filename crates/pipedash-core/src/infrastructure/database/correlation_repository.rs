mod applications;
mod artifacts;
mod changes;
mod edges;
mod enrichment;
mod observations;
mod overrides;
mod pipeline_level;
mod reset;
mod rows;
mod targets;
#[cfg(test)]
mod tests;
mod transitions;

use std::ops::{
    Deref,
    DerefMut,
};

use futures::future::BoxFuture;
pub use rows::{
    ApplicationMemberRow,
    ApplicationOverrideRow,
    ApplicationRow,
    ApplicationTargetRow,
    ArtifactObservationRow,
    ArtifactRow,
    ChangeRow,
    ChangeStageRow,
    CorrelationEdgeRow,
    LogScanRow,
    ObservationWindowRow,
    ObservedTransitionRow,
    RunEvidenceRow,
    SourceObservationRow,
    SubjectIdentityRow,
    TargetDirectionRow,
    TargetObservationRow,
    PIPELINE_LEVEL,
};
use rows::{
    Fetch,
    TableRow,
};
use sqlx::postgres::PgRow;
use sqlx::sqlite::SqliteRow;
use sqlx::{
    FromRow,
    Postgres,
    Sqlite,
    Transaction,
};

use super::bind::{
    bind_postgres,
    bind_sqlite,
    execute_bound,
    fetch_all_as,
    BindValue,
};
use super::pool::db_err;
use super::DatabasePool;
use crate::domain::{
    DomainError,
    DomainResult,
};

enum Inner {
    Pool(DatabasePool),
    Sqlite(Transaction<'static, Sqlite>),
    Postgres(Transaction<'static, Postgres>),
}

pub struct CorrelationConn {
    inner: Inner,
}

enum Scope<'a> {
    Owned(CorrelationConn),
    Borrowed(&'a mut CorrelationConn),
}

impl Deref for Scope<'_> {
    type Target = CorrelationConn;

    fn deref(&self) -> &CorrelationConn {
        match self {
            Scope::Owned(conn) => conn,
            Scope::Borrowed(conn) => conn,
        }
    }
}

impl DerefMut for Scope<'_> {
    fn deref_mut(&mut self) -> &mut CorrelationConn {
        match self {
            Scope::Owned(conn) => conn,
            Scope::Borrowed(conn) => conn,
        }
    }
}

impl Scope<'_> {
    async fn finish(self) -> DomainResult<()> {
        match self {
            Scope::Owned(conn) => conn.commit().await,
            Scope::Borrowed(_) => Ok(()),
        }
    }
}

pub struct CorrelationRepository {
    pool: DatabasePool,
}

impl CorrelationRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub fn conn(&self) -> CorrelationConn {
        CorrelationConn {
            inner: Inner::Pool(self.pool.clone()),
        }
    }

    pub async fn begin(&self) -> DomainResult<CorrelationConn> {
        let inner = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                Inner::Sqlite(pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_err)?)
            }
            DatabasePool::Postgres(pool) => Inner::Postgres(pool.begin().await.map_err(db_err)?),
        };
        Ok(CorrelationConn { inner })
    }

    pub async fn transaction<T, F>(&self, f: F) -> DomainResult<T>
    where
        T: Send,
        F: for<'c> FnOnce(&'c mut CorrelationConn) -> BoxFuture<'c, DomainResult<T>>,
    {
        let mut tx = self.begin().await?;
        match f(&mut tx).await {
            Ok(value) => tx.commit().await.map(|()| value),
            Err(error) => {
                if let Err(rollback) = tx.rollback().await {
                    tracing::warn!("Correlation rollback failed: {rollback}");
                }
                Err(error)
            }
        }
    }
}

impl CorrelationConn {
    pub async fn commit(self) -> DomainResult<()> {
        match self.inner {
            Inner::Pool(_) => Ok(()),
            Inner::Sqlite(tx) => tx.commit().await.map_err(db_err),
            Inner::Postgres(tx) => tx.commit().await.map_err(db_err),
        }
    }

    pub async fn rollback(self) -> DomainResult<()> {
        match self.inner {
            Inner::Pool(_) => Ok(()),
            Inner::Sqlite(tx) => tx.rollback().await.map_err(db_err),
            Inner::Postgres(tx) => tx.rollback().await.map_err(db_err),
        }
    }

    async fn scope(&mut self) -> DomainResult<Scope<'_>> {
        if let Inner::Pool(pool) = &self.inner {
            let tx = CorrelationRepository::new(pool.clone()).begin().await?;
            return Ok(Scope::Owned(tx));
        }
        Ok(Scope::Borrowed(self))
    }

    fn is_postgres(&self) -> bool {
        match &self.inner {
            Inner::Pool(pool) => pool.is_postgres(),
            Inner::Sqlite(_) => false,
            Inner::Postgres(_) => true,
        }
    }

    fn pick<T>(&self, sqlite: T, postgres: T) -> T {
        if self.is_postgres() {
            postgres
        } else {
            sqlite
        }
    }

    fn placeholder(&self, index: usize) -> String {
        if self.is_postgres() {
            format!("${index}")
        } else {
            "?".to_string()
        }
    }

    fn filter(&self, columns: &[&str]) -> String {
        if columns.is_empty() {
            return String::new();
        }
        let terms: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(index, column)| format!("{column} = {}", self.placeholder(index + 1)))
            .collect();
        format!(" WHERE {}", terms.join(" AND "))
    }

    async fn execute(&mut self, sql: &str, values: &[BindValue<'_>]) -> DomainResult<u64> {
        match &mut self.inner {
            Inner::Pool(pool) => execute_bound(pool, sql, values).await,
            Inner::Sqlite(tx) => bind_sqlite(sql, values)
                .execute(&mut **tx)
                .await
                .map(|result| result.rows_affected())
                .map_err(db_err),
            Inner::Postgres(tx) => bind_postgres(sql, values)
                .execute(&mut **tx)
                .await
                .map(|result| result.rows_affected())
                .map_err(db_err),
        }
    }

    async fn fetch<R: Fetch>(
        &mut self, sql: &str, values: &[BindValue<'_>],
    ) -> DomainResult<Vec<R>> {
        match &mut self.inner {
            Inner::Pool(pool) => fetch_all_as(pool, sql, values).await,
            Inner::Sqlite(tx) => bind_sqlite(sql, values)
                .fetch_all(&mut **tx)
                .await
                .map_err(db_err)?
                .iter()
                .map(|row: &SqliteRow| <R as FromRow<SqliteRow>>::from_row(row).map_err(db_err))
                .collect(),
            Inner::Postgres(tx) => bind_postgres(sql, values)
                .fetch_all(&mut **tx)
                .await
                .map_err(db_err)?
                .iter()
                .map(|row| <R as FromRow<PgRow>>::from_row(row).map_err(db_err))
                .collect(),
        }
    }

    async fn insert_returning_id(
        &mut self, sql: &str, values: &[BindValue<'_>],
    ) -> DomainResult<i64> {
        let rows: Vec<(i64,)> = self.fetch(sql, values).await?;
        rows.first()
            .map(|(id,)| *id)
            .ok_or_else(|| DomainError::DatabaseError("insert returned no id".to_string()))
    }

    async fn list<R: TableRow>(
        &mut self, columns: &[&str], values: &[BindValue<'_>],
    ) -> DomainResult<Vec<R>> {
        let sql = format!(
            "SELECT {} FROM {}{} ORDER BY {}",
            R::COLUMNS,
            R::TABLE,
            self.filter(columns),
            R::ORDER
        );
        self.fetch(&sql, values).await
    }

    async fn get<R: TableRow>(
        &mut self, columns: &[&str], values: &[BindValue<'_>],
    ) -> DomainResult<Option<R>> {
        Ok(self.list(columns, values).await?.into_iter().next())
    }

    async fn delete_where(
        &mut self, table: &str, columns: &[&str], values: &[BindValue<'_>],
    ) -> DomainResult<u64> {
        let sql = format!("DELETE FROM {table}{}", self.filter(columns));
        self.execute(&sql, values).await
    }
}
