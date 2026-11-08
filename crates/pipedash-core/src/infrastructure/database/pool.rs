use std::fmt::Display;

use sqlx::postgres::PgPool;
use sqlx::SqlitePool;

use crate::domain::DomainError;

pub(super) fn db_err(error: impl Display) -> DomainError {
    DomainError::DatabaseError(error.to_string())
}

#[derive(Clone)]
pub enum DatabasePool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

impl DatabasePool {
    pub fn is_postgres(&self) -> bool {
        matches!(self, DatabasePool::Postgres(_))
    }

    pub fn as_sqlite(&self) -> Option<&SqlitePool> {
        match self {
            DatabasePool::Sqlite(pool) => Some(pool),
            _ => None,
        }
    }

    pub(super) fn placeholder(&self, index: usize) -> String {
        match self {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${index}"),
        }
    }

    pub(super) fn pick<T>(&self, sqlite: T, postgres: T) -> T {
        match self {
            DatabasePool::Sqlite(_) => sqlite,
            DatabasePool::Postgres(_) => postgres,
        }
    }

    pub(super) fn datetime_now(&self) -> &'static str {
        match self {
            DatabasePool::Sqlite(_) => "datetime('now')",
            DatabasePool::Postgres(_) => "NOW()",
        }
    }

    pub(super) fn placeholders(&self, count: usize) -> String {
        (1..=count)
            .map(|index| self.placeholder(index))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(super) fn bigint(&self, column: &str) -> String {
        match self {
            DatabasePool::Sqlite(_) => column.to_string(),
            DatabasePool::Postgres(_) => format!("{column}::BIGINT"),
        }
    }
}
