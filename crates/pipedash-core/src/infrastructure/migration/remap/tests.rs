use chrono::{
    TimeZone,
    Utc,
};

use super::*;

#[test]
fn a_storage_migration_moves_mutes_and_watches_to_the_new_provider_ids_and_drops_removed_providers()
{
    let since = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap();
    let watch = |pipeline_id: &str| OnceWatch {
        pipeline_id: pipeline_id.into(),
        since,
        fired_at: None,
    };
    let preferences = NotificationPreferences {
        muted_providers: vec![2, 5],
        muted_pipelines: vec![
            "stub__2__acme/api__ci.yml".into(),
            "other__5__group/app".into(),
            "broken".into(),
        ],
        watched_pipelines: vec![
            "stub__2__acme/web__ci.yml".into(),
            "other__5__group/app".into(),
            "broken".into(),
        ],
        watch_once: vec![
            watch("stub__3__acme/api__ci.yml"),
            watch("other__5__group/app"),
            watch("broken"),
        ],
        ..NotificationPreferences::default()
    };
    let mapping = HashMap::from([(2, 1), (3, 2)]);

    assert_eq!(
        remap_preferences(preferences.clone(), &mapping),
        NotificationPreferences {
            muted_providers: vec![1],
            muted_pipelines: vec!["stub__1__acme/api__ci.yml".into()],
            watched_pipelines: vec!["stub__1__acme/web__ci.yml".into()],
            watch_once: vec![watch("stub__2__acme/api__ci.yml")],
            ..preferences
        }
    );
}

fn postgres_config(connection_string: &str) -> PipedashConfig {
    let mut config = PipedashConfig::default();
    config.storage.backend = crate::infrastructure::config::StorageBackend::Postgres;
    config.storage.postgres.connection_string = connection_string.into();
    config
}

#[test]
fn postgres_credentials_and_options_do_not_make_another_database() {
    let current = postgres_config("postgresql://app:old@db.internal:5432/pipedash");

    for edited in [
        "postgresql://app:new@db.internal:5432/pipedash",
        "postgresql://app:old@db.internal:5432/pipedash?application_name=pipedash",
        "postgresql://app:old@db.internal/pipedash?sslmode=require",
        "postgres://app:old@DB.internal:5432/pipedash",
    ] {
        assert!(
            same_database(&current, &postgres_config(edited)),
            "{edited}"
        );
    }
}

#[test]
fn postgres_host_port_or_database_name_makes_another_database() {
    let current = postgres_config("postgresql://app:old@db.internal:5432/pipedash");

    for moved in [
        "postgresql://app:old@db.internal:5432/pipedash_next",
        "postgresql://app:old@db.internal:5433/pipedash",
        "postgresql://app:old@replica.internal:5432/pipedash",
        "postgresql://app:old@db.internal:5432/pipedash?dbname=pipedash_next",
        "not a connection string",
    ] {
        assert!(!same_database(&current, &postgres_config(moved)), "{moved}");
    }
}
