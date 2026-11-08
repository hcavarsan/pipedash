use std::collections::HashMap;

use chrono::Utc;

use super::DeviceRepository;
use crate::domain::{
    NotificationPreferences,
    PairedDevice,
};
use crate::infrastructure::database::{
    init_database,
    DatabasePool,
};

#[tokio::test]
async fn sqlite_devices_are_found_by_token_touched_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_database(dir.path().join("devices.db")).await.unwrap();
    let repository = DeviceRepository::new(DatabasePool::Sqlite(pool));
    let (device, token) = PairedDevice::issue("Pixel", "android");

    repository.insert(&device).await.unwrap();

    let found = repository.find_by_token(&token).await.unwrap().unwrap();
    assert_eq!(found.id, device.id);
    assert!(found.last_seen_at.is_some());
    assert_eq!(repository.find_by_token("wrong").await.unwrap(), None);
    assert_eq!(
        repository.find_by_token(&device.token_hash).await.unwrap(),
        None
    );

    assert!(repository
        .notification_preferences()
        .await
        .unwrap()
        .is_empty());
    let muted = NotificationPreferences {
        long_run: false,
        muted_pipelines: vec!["stub__1__api".into()],
        ..NotificationPreferences::default()
    };
    assert!(repository
        .set_notification_preferences(&device.id, &muted)
        .await
        .unwrap());
    assert!(!repository
        .set_notification_preferences("unknown", &muted)
        .await
        .unwrap());
    assert_eq!(
        repository.notification_preferences().await.unwrap(),
        HashMap::from([(device.id.clone(), muted)])
    );

    repository.touch(&device.id).await.unwrap();
    let listed = repository.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].last_seen_at.is_some());

    assert!(repository.delete(&device.id).await.unwrap());
    assert!(!repository.delete(&device.id).await.unwrap());
    assert_eq!(repository.find_by_token(&token).await.unwrap(), None);

    repository.import(&listed).await.unwrap();
    repository.import(&listed).await.unwrap();
    assert_eq!(repository.list().await.unwrap(), listed);
    assert_eq!(repository.delete_all().await.unwrap(), 1);
    assert!(repository.list().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_saved_between_read_and_write_is_kept_and_reapplied() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_database(dir.path().join("devices.db")).await.unwrap();
    let repository = DeviceRepository::new(DatabasePool::Sqlite(pool));
    let (device, _) = PairedDevice::issue("Pixel", "android");
    repository.insert(&device).await.unwrap();
    let request = crate::domain::OnceWatch {
        pipeline_id: "stub__1__api".into(),
        since: Utc::now(),
        fired_at: None,
    };
    let finish = crate::domain::RunFinish {
        at: request.since + chrono::Duration::minutes(1),
        previous: None,
    };
    let incoming = NotificationPreferences {
        outage: false,
        watch_once: vec![request],
        ..NotificationPreferences::default()
    };
    repository
        .set_notification_preferences(&device.id, &incoming)
        .await
        .unwrap();
    let mut attempts = 0;

    let saved = repository
        .update_notification_preferences(&device.id, |stored| {
            attempts += 1;
            if attempts == 1 {
                let mut fired = stored.clone();
                assert!(!fired.fire_once("stub__1__api", &finish).is_empty());
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(repository.set_notification_preferences(&device.id, &fired))
                })
                .unwrap();
            }
            let mut next = incoming.clone();
            next.keep_fired(stored);
            *stored = next;
            true
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(attempts, 2);
    assert_eq!(saved.watch_once[0].fired_at, Some(finish.at));
    assert_eq!(
        repository.notification_preferences().await.unwrap()[&device.id],
        saved
    );
    assert_eq!(
        repository
            .update_notification_preferences("unknown", |_| true)
            .await
            .unwrap(),
        None
    );
}
