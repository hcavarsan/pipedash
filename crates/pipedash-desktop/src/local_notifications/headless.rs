use std::path::Path;
use std::sync::{
    Mutex,
    PoisonError,
};

use pipedash_core::application::{
    refresh_notifications,
    LocalNotification,
    NotificationState,
};
use tokio::sync::watch;

use super::{
    store,
    wake,
    REFRESH_BUDGET,
    REFRESH_TIMEOUT,
};

static ACTIVE: Mutex<Option<watch::Sender<bool>>> = Mutex::new(None);

pub fn cancel() {
    let active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(cancel) = active {
        let _ = cancel.send(true);
    }
}

pub fn run(data_dir: &Path) -> Vec<LocalNotification> {
    store::set_dir(data_dir.join("background-notifications"));
    let Ok(generation) = store::generation() else {
        return Vec::new();
    };
    let authorization = store::authorization().ok().flatten();
    let host = store::alert_host().ok().flatten();
    if authorization.is_none() && host.is_none() {
        return Vec::new();
    }
    let Ok(preferences) = store::preferences() else {
        return Vec::new();
    };
    let mut observations = if authorization.is_some() {
        let Ok(observations) = store::observations() else {
            return Vec::new();
        };
        observations
    } else {
        NotificationState::default()
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Vec::new();
    };
    let (cancel, mut cancelled) = watch::channel(false);
    *ACTIVE.lock().unwrap_or_else(PoisonError::into_inner) = Some(cancel);
    let wake = runtime.block_on(async {
        tokio::select! {
            biased;
            _ = cancelled.changed() => None,
            result = tokio::time::timeout(
                REFRESH_TIMEOUT,
                wake::gather(
                    async {
                        match &authorization {
                            Some(authorization) => refresh_notifications(
                                &authorization.providers,
                                &authorization.proxy_url,
                                authorization.status_pages_enabled,
                                &mut observations,
                                REFRESH_BUDGET,
                            )
                            .await
                            .ok(),
                            None => None,
                        }
                    },
                    async {
                        match &host {
                            Some(host) => wake::fetch_host(host, REFRESH_BUDGET).await,
                            None => None,
                        }
                    },
                ),
            ) => result.ok(),
        }
    });
    ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).take();
    let Some(mut wake) = wake else {
        return Vec::new();
    };

    let mut notices = Vec::new();
    if let Some(scanned) = wake.scanned.take() {
        if matches!(
            store::save_observations_if_unchanged(&observations, generation),
            Ok(true)
        ) {
            notices = scanned;
        }
    }
    if let (Some(mut batch), Some(host)) = (wake.hosted.take(), host.as_ref()) {
        if matches!(wake::save_cursor(host, &batch), Ok(true)) {
            if let Some(through) = batch.ack {
                runtime.block_on(wake::acknowledge(host, through));
            }
            notices.append(&mut batch.notices);
        }
    }

    let shown = super::wanted(&preferences, notices);
    let finishes: Vec<_> = shown
        .iter()
        .filter_map(|notice| notice.pipeline_id.clone().zip(notice.finish))
        .collect();
    if !finishes.is_empty() {
        let _sync = super::PREFERENCES_SYNC.blocking_lock();
        if store::fire_once(&finishes).is_err() {
            tracing::warn!("Couldn't mark a one-time watch as used");
        }
    }

    shown
}
