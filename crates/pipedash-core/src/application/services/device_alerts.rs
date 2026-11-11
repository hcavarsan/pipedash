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
            pipelines.cached_pipelines(None).await?,
            providers.list_providers().await?,
        ))
    };
    match cached.await {
        Ok((pipelines, providers)) => state.seed(&pipelines, &providers),
        Err(error) => tracing::warn!(%error, "Failed to seed the alert recorder"),
    }

    state
}

pub struct Recorder {
    pub alerts: DeviceAlertRepository,
    pub devices: DeviceRepository,
    pub bus: Arc<dyn EventBus>,
    pub recent: Arc<RecentNotifications>,
}

pub async fn record(
    rx: mpsc::Receiver<CoreEvent>, state: NotificationState, stop: CancellationToken,
    recorder: Recorder,
) {
    record_pruning(rx, state, stop, recorder, PRUNE_INTERVAL).await;
}

async fn record_pruning(
    mut rx: mpsc::Receiver<CoreEvent>, mut state: NotificationState, stop: CancellationToken,
    recorder: Recorder, prune_every: Duration,
) {
    let Recorder {
        alerts,
        devices,
        bus,
        recent,
    } = recorder;
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
mod tests;
