use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::{
    self,
    error::TrySendError,
};
use tokio_util::sync::CancellationToken;

use super::health_service::NotificationState;
use super::pipeline_service::PipelineService;
use super::provider_service::ProviderService;
use crate::domain::{
    DomainError,
    NewDeviceAlert,
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
) {
    record_pruning(rx, state, stop, alerts, devices, bus, PRUNE_INTERVAL).await;
}

async fn record_pruning(
    mut rx: mpsc::Receiver<CoreEvent>, mut state: NotificationState, stop: CancellationToken,
    alerts: DeviceAlertRepository, devices: DeviceRepository, bus: Arc<dyn EventBus>,
    prune_every: Duration,
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
        match devices.list().await {
            Ok(paired) if paired.is_empty() => continue,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "Failed to list paired devices for alerts");
                continue;
            }
        }
        for notice in notices {
            let alert = NewDeviceAlert {
                key: notice.key,
                kind: notice.kind,
                title: notice.title,
                body: notice.body,
                pipeline_id: notice.pipeline_id,
                provider_id: notice.provider_id,
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
}
