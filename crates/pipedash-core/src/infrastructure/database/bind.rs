use chrono::{
    DateTime,
    Utc,
};
use sqlx::postgres::{
    PgArguments,
    PgRow,
};
use sqlx::query::Query;
use sqlx::sqlite::{
    SqliteArguments,
    SqliteRow,
};
use sqlx::{
    FromRow,
    Postgres,
    Sqlite,
};

use super::pool::db_err;
use super::DatabasePool;
use crate::domain::DomainResult;

pub(super) enum BindValue<'a> {
    Text(&'a str),
    OptText(Option<&'a str>),
    Int(i64),
    OptInt(Option<i64>),
    Bool(bool),
    Timestamp(DateTime<Utc>),
    OptTimestamp(Option<DateTime<Utc>>),
}

macro_rules! bind_values {
    ($query:expr, $values:expr) => {
        $values.iter().fold($query, |query, value| match value {
            BindValue::Text(text) => query.bind(*text),
            BindValue::OptText(text) => query.bind(*text),
            BindValue::Int(int) => query.bind(*int),
            BindValue::OptInt(int) => query.bind(*int),
            BindValue::Bool(flag) => query.bind(*flag),
            BindValue::Timestamp(at) => query.bind(*at),
            BindValue::OptTimestamp(at) => query.bind(*at),
        })
    };
}

pub(super) fn bind_sqlite<'q>(
    sql: &'q str, values: &[BindValue<'q>],
) -> Query<'q, Sqlite, SqliteArguments> {
    bind_values!(sqlx::query(sqlx::AssertSqlSafe(sql)), values)
}

pub(super) fn bind_postgres<'q>(
    sql: &'q str, values: &[BindValue<'q>],
) -> Query<'q, Postgres, PgArguments> {
    bind_values!(sqlx::query(sqlx::AssertSqlSafe(sql)), values)
}

pub(super) async fn execute_bound(
    pool: &DatabasePool, sql: &str, values: &[BindValue<'_>],
) -> DomainResult<u64> {
    match pool {
        DatabasePool::Sqlite(p) => bind_sqlite(sql, values)
            .execute(p)
            .await
            .map(|result| result.rows_affected()),
        DatabasePool::Postgres(p) => bind_postgres(sql, values)
            .execute(p)
            .await
            .map(|result| result.rows_affected()),
    }
    .map_err(db_err)
}

pub(super) async fn fetch_rows<T>(
    pool: &DatabasePool, sql: &str, values: &[BindValue<'_>],
    sqlite: impl Fn(&SqliteRow) -> DomainResult<T>, postgres: impl Fn(&PgRow) -> DomainResult<T>,
) -> DomainResult<Vec<T>> {
    match pool {
        DatabasePool::Sqlite(p) => bind_sqlite(sql, values)
            .fetch_all(p)
            .await
            .map_err(db_err)?
            .iter()
            .map(sqlite)
            .collect(),
        DatabasePool::Postgres(p) => bind_postgres(sql, values)
            .fetch_all(p)
            .await
            .map_err(db_err)?
            .iter()
            .map(postgres)
            .collect(),
    }
}

pub(super) async fn fetch_all_as<R>(
    pool: &DatabasePool, sql: &str, values: &[BindValue<'_>],
) -> DomainResult<Vec<R>>
where
    R: for<'r> FromRow<'r, SqliteRow> + for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    on_pool!(pool, p => bind_values!(sqlx::query_as::<_, R>(sqlx::AssertSqlSafe(sql)), values)
        .fetch_all(p)
        .await)
    .map_err(db_err)
}

pub(super) async fn fetch_optional_as<R>(
    pool: &DatabasePool, sql: &str, values: &[BindValue<'_>],
) -> DomainResult<Option<R>>
where
    R: for<'r> FromRow<'r, SqliteRow> + for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    on_pool!(pool, p => bind_values!(sqlx::query_as::<_, R>(sqlx::AssertSqlSafe(sql)), values)
        .fetch_optional(p)
        .await)
    .map_err(db_err)
}

pub(super) async fn set_timestamp(
    pool: &DatabasePool, table: &str, column: &str, key_column: &str, key: &str, at: DateTime<Utc>,
) -> DomainResult<()> {
    let sql = format!(
        "UPDATE {table} SET {column} = {} WHERE {key_column} = {}",
        pool.placeholder(1),
        pool.placeholder(2)
    );
    execute_bound(
        pool,
        &sql,
        &[BindValue::Timestamp(at), BindValue::Text(key)],
    )
    .await?;

    Ok(())
}

pub(super) async fn delete_by_key(
    pool: &DatabasePool, table: &str, key_column: &str, key: &str,
) -> DomainResult<bool> {
    let sql = format!(
        "DELETE FROM {table} WHERE {key_column} = {}",
        pool.placeholder(1)
    );

    Ok(execute_bound(pool, &sql, &[BindValue::Text(key)]).await? > 0)
}
