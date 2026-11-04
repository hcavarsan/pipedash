use std::collections::{
    HashMap,
    VecDeque,
};
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::mpsc::{
    self,
    error::TrySendError,
};
use tokio_util::sync::CancellationToken;

use super::health_service::{
    LocalNotification,
    NotificationState,
    ObservedNotification,
};
use super::pipeline_service::PipelineService;
use super::provider_service::ProviderService;
use crate::domain::{
    AlertKind,
    DomainError,
    NewDeviceAlert,
    NotificationPreferences,
    RunFinish,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::{
    DeviceAlertRepository,
    DeviceRepository,
};

const CHANNEL_CAPACITY: usize = 512;
const PRUNE_INTERVAL: Duration = Duration::from_secs(3600);
const RECENT_LIMIT: usize = 256;

pub struct RecentNotifications {
    entries: Mutex<VecDeque<ObservedNotification>>,
    next_id: AtomicU64,
}

impl Default for RecentNotifications {
    fn default() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(RECENT_LIMIT)),
            next_id: AtomicU64::new(
                u64::try_from(Utc::now().timestamp_micros()).unwrap_or_default(),
            ),
        }
    }
}

impl RecentNotifications {
    fn observe(&self, notice: LocalNotification) -> ObservedNotification {
        let observed = ObservedNotification {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            at: Utc::now(),
            notice,
        };
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() == RECENT_LIMIT {
            entries.pop_front();
        }
        entries.push_back(observed.clone());

        observed
    }

    pub fn after(&self, after: u64) -> Vec<ObservedNotification> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|observed| observed.id > after)
            .cloned()
            .collect()
    }
}

struct RecordingBus {
    inner: Arc<dyn EventBus>,
    tx: mpsc::Sender<CoreEvent>,
}

fn is_observed(event: &CoreEvent) -> bool {
    matches!(
        event,
        CoreEvent::PipelinesUpdated { .. }
            | CoreEvent::PipelinesFailed { .. }
            | CoreEvent::ProviderHealthAlert { .. }
            | CoreEvent::ProviderAdded { .. }
            | CoreEvent::ProviderUpdated { .. }
            | CoreEvent::ProviderStatusUpdated { .. }
            | CoreEvent::ProviderRemoved { .. }
    )
}

#[async_trait]
impl EventBus for RecordingBus {
    async fn emit(&self, event: CoreEvent) {
        if is_observed(&event) {
            if let Err(TrySendError::Full(_)) = self.tx.try_send(event.clone()) {
                tracing::debug!("Alert queue is full; an observation was dropped");
            }
        }
        self.inner.emit(event).await;
    }

    async fn emit_to(&self, target: &str, event: CoreEvent) {
        self.inner.emit_to(target, event).await;
    }
}

pub fn recording_bus(inner: Arc<dyn EventBus>) -> (Arc<dyn EventBus>, mpsc::Receiver<CoreEvent>) {
    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

    (Arc::new(RecordingBus { inner, tx }), rx)
}

pub async fn seed(pipelines: &PipelineService, providers: &ProviderService) -> NotificationState {
    let mut state = NotificationState::default();
    let cached = async {
        Ok::<_, DomainError>((
            pipelines.get_cached_pipelines(None).await?,
            providers.list_providers().await?,
        ))
    };
    match cached.await {
        Ok((pipelines, providers)) => state.seed(&pipelines, &providers),
        Err(error) => tracing::warn!(%error, "Failed to seed the alert recorder"),
    }

    state
}

pub async fn record(
    rx: mpsc::Receiver<CoreEvent>, state: NotificationState, stop: CancellationToken,
    alerts: DeviceAlertRepository, devices: DeviceRepository, bus: Arc<dyn EventBus>,
    recent: Arc<RecentNotifications>,
) {
    record_pruning(
        rx,
        state,
        stop,
        alerts,
        devices,
        bus,
        recent,
        PRUNE_INTERVAL,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn record_pruning(
    mut rx: mpsc::Receiver<CoreEvent>, mut state: NotificationState, stop: CancellationToken,
    alerts: DeviceAlertRepository, devices: DeviceRepository, bus: Arc<dyn EventBus>,
    recent: Arc<RecentNotifications>, prune_every: Duration,
) {
    let mut prune = tokio::time::interval(prune_every);
    prune.tick().await;
    loop {
        let event = tokio::select! {
            biased;
            _ = stop.cancelled() => break,
            _ = prune.tick() => {
                if let Err(error) = alerts.prune().await {
                    tracing::warn!(%error, "Failed to prune old device alerts");
                }
                continue;
            }
            event = rx.recv() => match event {
                Some(event) => event,
                None => break,
            },
        };
        let notices = state.observe_event(&event);
        if notices.is_empty() {
            continue;
        }
        for notice in &notices {
            bus.emit(CoreEvent::NotificationObserved {
                notification: recent.observe(notice.clone()),
            })
            .await;
        }
        match devices.list().await {
            Ok(paired) if paired.is_empty() => continue,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "Failed to list paired devices for alerts");
                continue;
            }
        }
        let mut watchers = if notices.iter().any(|notice| notice.finish.is_some()) {
            match devices.notification_preferences().await {
                Ok(preferences) => preferences,
                Err(error) => {
                    tracing::warn!(%error, "Failed to load who watches finished runs");
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };
        for notice in notices {
            if notice.kind == AlertKind::Finished && !wanted_by_any(&watchers, &notice) {
                continue;
            }
            if let (Some(pipeline_id), Some(finish)) = (&notice.pipeline_id, &notice.finish) {
                fire_once(&devices, &mut watchers, pipeline_id, finish).await;
            }
            let alert = NewDeviceAlert {
                key: notice.key,
                kind: notice.kind,
                title: notice.title,
                body: notice.body,
                pipeline_id: notice.pipeline_id,
                provider_id: notice.provider_id,
                provider_ids: notice.provider_ids,
                finish: notice.finish,
            };
            match alerts.insert(&alert).await {
                Ok(stored) => {
                    bus.emit(CoreEvent::AlertRecorded {
                        id: stored.id,
                        seq: stored.seq,
                    })
                    .await;
                }
                Err(error) => tracing::warn!(%error, "Failed to record a device alert"),
            }
        }
    }
}

fn wanted_by_any(
    watchers: &HashMap<String, NotificationPreferences>, notice: &LocalNotification,
) -> bool {
    watchers.values().any(|preferences| {
        preferences.allows(
            notice.kind,
            &notice.provider_ids,
            notice.pipeline_id.as_deref(),
            notice.finish.as_ref(),
        )
    })
}

async fn fire_once(
    devices: &DeviceRepository, watchers: &mut HashMap<String, NotificationPreferences>,
    pipeline_id: &str, finish: &RunFinish,
) {
    for (device_id, preferences) in watchers.iter_mut() {
        if preferences.fire_once(pipeline_id, finish).is_empty() {
            continue;
        }
        match devices
            .update_notification_preferences(device_id, |stored| {
                !stored.fire_once(pipeline_id, finish).is_empty()
            })
            .await
        {
            Ok(Some(saved)) => *preferences = saved,
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "Failed to mark a one-time watch as used"),
        }
    }
}

#[cfg(test)]
mod tests {
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
            DeviceAlertRepository::new(pool.clone()),
            devices,
            h.bus.clone(),
            Arc::default(),
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
            DeviceAlertRepository::new(pool.clone()),
            DeviceRepository::new(pool),
            h.bus.clone(),
            Arc::default(),
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
            DeviceAlertRepository::new(pool.clone()),
            DeviceRepository::new(pool),
            h.bus.clone(),
            Arc::default(),
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
        assert_eq!(stored[0].pipeline_id.as_deref(), Some("github__1__api"));
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
                    watched_pipelines: vec!["github__1__api".into()],
                    ..NotificationPreferences::default()
                },
            )
            .await
            .unwrap();
        let stored = recorded(&h, false, finishing(2)).await;

        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].kind, AlertKind::Finished);
        assert_eq!(stored[0].title, "api succeeded");
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
                        pipeline_id: "github__1__api".into(),
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
            Some("github__1__api"),
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
            DeviceAlertRepository::new(DatabasePool::Sqlite(h.pool.clone())),
            DeviceRepository::new(DatabasePool::Sqlite(h.pool.clone())),
            h.bus.clone(),
            Arc::clone(&recent),
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
}
