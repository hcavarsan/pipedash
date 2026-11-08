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
