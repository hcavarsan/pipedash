#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{
    DateTime,
    Utc,
};
use serde_json::json;

#[tokio::test]
async fn sqlite_migration_backfills_the_order_key_of_cached_flux_runs() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE run_history_cache (
            pipeline_id TEXT NOT NULL,
            run_number INTEGER NOT NULL,
            run_data TEXT NOT NULL,
            PRIMARY KEY (pipeline_id, run_number)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    let timestamps = [
        "2026-10-21T10:00:00Z",
        "2026-10-21T10:00:00.1Z",
        "2026-10-21T10:00:00.500999Z",
        "2026-10-21T10:00:00.999999Z",
    ];
    let pipelines = [
        "fluxcd__1__Kustomization__apps__web",
        "github__1__acme__web",
    ];
    for pipeline_id in pipelines {
        for (index, timestamp) in timestamps.iter().enumerate() {
            sqlx::query("INSERT INTO run_history_cache (pipeline_id, run_number, run_data) VALUES (?, ?, ?)")
                .bind(pipeline_id)
                .bind((index + 1) as i64)
                .bind(json!({ "started_at": timestamp }).to_string())
                .execute(&pool).await.unwrap();
        }
    }

    sqlx::raw_sql(include_str!(
        "../../pipedash-core/migrations/sqlite/20261021000001_run_history_started_at.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let keys: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT pipeline_id, run_number, started_at_ms FROM run_history_cache ORDER BY pipeline_id, run_number",
    ).fetch_all(&pool).await.unwrap();
    let expected: Vec<_> = pipelines
        .iter()
        .flat_map(|pipeline_id| {
            timestamps
                .iter()
                .enumerate()
                .map(move |(index, timestamp)| {
                    let millis = if pipeline_id.starts_with("fluxcd__") {
                        timestamp
                            .parse::<DateTime<Utc>>()
                            .unwrap()
                            .timestamp_millis()
                    } else {
                        0
                    };
                    (pipeline_id.to_string(), (index + 1) as i64, millis)
                })
        })
        .collect();
    assert_eq!(keys, expected);
}
