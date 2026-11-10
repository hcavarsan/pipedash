use chrono::{
    Duration,
    TimeZone,
    Utc,
};

use super::*;
use crate::application::test_support::{
    pipeline,
    Harness,
};
use crate::domain::{
    AlertKind,
    DeviceAlert,
    PairedDevice,
    Pipeline,
    PipelineStatus,
};
use crate::infrastructure::database::DatabasePool;

fn run(status: PipelineStatus, minute: i64) -> Pipeline {
    let mut pipeline = pipeline(1, "api", status);
    pipeline.last_run =
        Some(Utc.with_ymd_and_hms(2026, 11, 3, 12, 0, 0).unwrap() + Duration::minutes(minute));
    pipeline
}

fn updated(pipeline: Pipeline) -> CoreEvent {
    CoreEvent::PipelinesUpdated {
        pipelines: vec![pipeline],
        provider_id: 1,
        timestamp: 0,
    }
}

async fn recorded(h: &Harness, paired: bool, events: Vec<CoreEvent>) -> Vec<DeviceAlert> {
    let pool = DatabasePool::Sqlite(h.pool.clone());
    let devices = DeviceRepository::new(pool.clone());
    if paired {
        devices
            .insert(&PairedDevice::issue("Pixel", "android").0)
            .await
            .unwrap();
    }
    let (bus, rx) = recording_bus(h.bus.clone());
    let state = seed(&h.pipeline_service, &h.provider_service).await;
    let task = tokio::spawn(record(
        rx,
        state,
        CancellationToken::new(),
        Recorder {
            alerts: DeviceAlertRepository::new(pool.clone()),
            devices,
            bus: h.bus.clone(),
            recent: Arc::default(),
        },
    ));
    for event in events {
        bus.emit(event).await;
    }
    drop(bus);
    task.await.unwrap();

    DeviceAlertRepository::new(pool).list().await.unwrap()
}

#[tokio::test]
async fn the_recorder_stops_when_cancelled_while_events_can_still_arrive() {
    let h = Harness::new().await;
    let pool = DatabasePool::Sqlite(h.pool.clone());
    let (bus, rx) = recording_bus(h.bus.clone());
    let stop = CancellationToken::new();
    let task = tokio::spawn(record(
        rx,
        NotificationState::default(),
        stop.clone(),
        Recorder {
            alerts: DeviceAlertRepository::new(pool.clone()),
            devices: DeviceRepository::new(pool),
            bus: h.bus.clone(),
            recent: Arc::default(),
        },
    ));

    stop.cancel();

    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("recorder kept running after cancel")
        .unwrap();
    drop(bus);
}

#[tokio::test]
async fn the_recorder_prunes_expired_alerts_periodically() {
    let h = Harness::new().await;
    let pool = DatabasePool::Sqlite(h.pool.clone());
    let alerts = DeviceAlertRepository::new(pool.clone());
    let stored = alerts
        .insert(&NewDeviceAlert {
            key: "pipeline:p".into(),
            kind: AlertKind::Failed,
            title: "t".into(),
            body: "b".into(),
            pipeline_id: None,
            provider_id: None,
            provider_ids: Vec::new(),
            finish: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE device_alerts SET created_at = ? WHERE seq = ?")
        .bind((Utc::now() - Duration::days(8)).to_rfc3339())
        .bind(stored.seq)
        .execute(&h.pool)
        .await
        .unwrap();
    let (_bus, rx) = recording_bus(h.bus.clone());
    let stop = CancellationToken::new();
    let task = tokio::spawn(record_pruning(
        rx,
        NotificationState::default(),
        stop.clone(),
        Recorder {
            alerts: DeviceAlertRepository::new(pool.clone()),
            devices: DeviceRepository::new(pool),
            bus: h.bus.clone(),
            recent: Arc::default(),
        },
        std::time::Duration::from_millis(300),
    ));

    assert_eq!(alerts.list().await.unwrap().len(), 1);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !alerts.list().await.unwrap().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "expired alert was never pruned"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    stop.cancel();
    task.await.unwrap();
}

#[tokio::test]
async fn the_seed_reflects_the_cached_pipelines() {
    let h = Harness::new().await;
    assert_eq!(h.add_provider("ci").await, 1);
    h.repository
        .update_pipelines_cache(1, &[run(PipelineStatus::Success, 0)])
        .await
        .unwrap();

    let stored = recorded(&h, true, vec![updated(run(PipelineStatus::Failed, 1))]).await;

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].kind, AlertKind::Failed);
}

#[tokio::test]
async fn a_failing_pipeline_is_recorded_once_and_announced() {
    let h = Harness::new().await;
    let events = vec![
        updated(run(PipelineStatus::Success, 0)),
        updated(run(PipelineStatus::Failed, 1)),
    ];

    let stored = recorded(&h, true, events).await;

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].kind, AlertKind::Failed);
    assert_eq!(stored[0].pipeline_id.as_deref(), Some("stub__1__api"));
    let announced = h
        .bus
        .events()
        .into_iter()
        .filter_map(|event| match event {
            CoreEvent::AlertRecorded { id, seq } => Some((id, seq)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(announced, vec![(stored[0].id.clone(), stored[0].seq)]);
}

#[tokio::test]
async fn a_pipeline_that_passes_again_is_recorded_once_as_a_recovery_with_its_finish() {
    let h = Harness::new().await;
    let events = vec![
        updated(run(PipelineStatus::Failed, 0)),
        updated(run(PipelineStatus::Running, 1)),
        updated(run(PipelineStatus::Success, 1)),
        updated(run(PipelineStatus::Success, 1)),
    ];

    let stored = recorded(&h, true, events).await;

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].kind, AlertKind::Recovered);
    assert_eq!(stored[0].title, "api passed again · acme/app");
    assert_eq!(stored[0].pipeline_id.as_deref(), Some("stub__1__api"));
    assert!(stored[0].finish.is_some());
}

#[tokio::test]
async fn nothing_is_recorded_without_a_paired_device() {
    let h = Harness::new().await;
    let events = vec![
        updated(run(PipelineStatus::Success, 0)),
        updated(run(PipelineStatus::Failed, 1)),
    ];

    assert!(recorded(&h, false, events).await.is_empty());
    assert_eq!(h.bus.count("alert-recorded"), 0);
}

#[tokio::test]
async fn the_first_sight_of_a_failed_pipeline_is_not_recorded() {
    let h = Harness::new().await;
    let events = vec![updated(run(PipelineStatus::Failed, 1))];

    assert!(recorded(&h, true, events).await.is_empty());
}

#[tokio::test]
async fn a_finished_run_is_recorded_only_while_a_device_watches_it() {
    let h = Harness::new().await;
    let devices = DeviceRepository::new(DatabasePool::Sqlite(h.pool.clone()));
    let device = PairedDevice::issue("Pixel", "android").0;
    devices.insert(&device).await.unwrap();
    let finishing = |minute| {
        vec![
            updated(run(PipelineStatus::Running, minute)),
            updated(run(PipelineStatus::Success, minute)),
        ]
    };

    assert!(recorded(&h, false, finishing(1)).await.is_empty());

    devices
        .set_notification_preferences(
            &device.id,
            &NotificationPreferences {
                watched_pipelines: vec!["stub__1__api".into()],
                ..NotificationPreferences::default()
            },
        )
        .await
        .unwrap();
    let stored = recorded(&h, false, finishing(2)).await;

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].kind, AlertKind::Finished);
    assert_eq!(stored[0].title, "api succeeded · acme/app");
    assert!(stored[0].finish.is_some());
}

#[tokio::test]
async fn a_one_time_watch_is_used_once_even_when_the_server_restarts() {
    let h = Harness::new().await;
    let devices = DeviceRepository::new(DatabasePool::Sqlite(h.pool.clone()));
    let device = PairedDevice::issue("Pixel", "android").0;
    devices.insert(&device).await.unwrap();
    devices
        .set_notification_preferences(
            &device.id,
            &NotificationPreferences {
                watch_once: vec![crate::domain::OnceWatch {
                    pipeline_id: "stub__1__api".into(),
                    since: Utc::now() - Duration::minutes(1),
                    fired_at: None,
                }],
                ..NotificationPreferences::default()
            },
        )
        .await
        .unwrap();
    let finishing = |minute| {
        vec![
            updated(run(PipelineStatus::Running, minute)),
            updated(run(PipelineStatus::Success, minute)),
        ]
    };

    let first = recorded(&h, false, finishing(1)).await;
    let after_restart = recorded(&h, false, finishing(2)).await;

    assert_eq!(first.len(), 1);
    assert_eq!(after_restart, first);
    let saved = devices.notification_preferences().await.unwrap()[&device.id].clone();
    assert_eq!(
        saved.watch_once[0].fired_at,
        first[0].finish.map(|finish| finish.at)
    );
    assert!(saved.allows(
        AlertKind::Finished,
        &[1],
        Some("stub__1__api"),
        first[0].finish.as_ref()
    ));
}

#[tokio::test]
async fn every_notice_is_announced_with_a_rising_id_even_without_a_paired_device() {
    let h = Harness::new().await;
    let recent = Arc::new(RecentNotifications::default());
    let (bus, rx) = recording_bus(h.bus.clone());
    let state = seed(&h.pipeline_service, &h.provider_service).await;
    let task = tokio::spawn(record(
        rx,
        state,
        CancellationToken::new(),
        Recorder {
            alerts: DeviceAlertRepository::new(DatabasePool::Sqlite(h.pool.clone())),
            devices: DeviceRepository::new(DatabasePool::Sqlite(h.pool.clone())),
            bus: h.bus.clone(),
            recent: Arc::clone(&recent),
        },
    ));
    for event in [
        updated(run(PipelineStatus::Running, 0)),
        updated(run(PipelineStatus::Success, 0)),
        updated(run(PipelineStatus::Failed, 1)),
    ] {
        bus.emit(event).await;
    }
    drop(bus);
    task.await.unwrap();

    let announced: Vec<ObservedNotification> = h
        .bus
        .events()
        .into_iter()
        .filter_map(|event| match event {
            CoreEvent::NotificationObserved { notification } => Some(notification),
            _ => None,
        })
        .collect();
    assert_eq!(
        announced
            .iter()
            .map(|observed| observed.notice.kind)
            .collect::<Vec<_>>(),
        [AlertKind::Finished, AlertKind::Failed]
    );
    assert!(announced[0].id < announced[1].id);
    assert_eq!(recent.after(announced[0].id), announced[1..]);
    assert_eq!(h.bus.count("alert-recorded"), 0);
}
