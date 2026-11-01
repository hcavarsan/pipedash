use std::{
    collections::HashMap,
    sync::{
        Mutex,
        MutexGuard,
        PoisonError,
    },
};

use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
    State,
    Url,
};
use tauri_plugin_pipedash_native::{
    Accessibility,
    Appearance,
    HapticKind,
    NotificationResponse,
    PipedashNativeExt,
    PrivacyCover,
    ShareRect,
};

use crate::{
    app_lock::AppLockSettings,
    commands::ErrorResponse,
};

pub const INTENT_EVENT: &str = "native-intent";
pub const STATUS_BAR_TAP_EVENT: &str = "native-status-bar-tap";
#[cfg(mobile)]
pub const FAILURE_CATEGORY: &str = "pipedash-failure";

const DEEP_LINK_SCHEME: &str = "pipedash";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_number: Option<u64>,
}

impl NotificationTarget {
    pub fn extra(&self) -> Vec<(&'static str, String)> {
        [
            ("key", self.key.clone()),
            ("pipelineId", self.pipeline_id.clone()),
            ("providerId", self.provider_id.map(|id| id.to_string())),
            ("runNumber", self.run_number.map(|run| run.to_string())),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name, value)))
        .collect()
    }

    pub fn from_extra(extra: &HashMap<String, Value>) -> Self {
        let text = |name: &str| extra.get(name).and_then(Value::as_str);
        Self {
            key: text("key").map(str::to_owned),
            pipeline_id: text("pipelineId").map(str::to_owned),
            provider_id: text("providerId").and_then(|id| id.parse().ok()),
            run_number: text("runNumber").and_then(|run| run.parse().ok()),
        }
    }

    pub fn from_response(response: &NotificationResponse) -> Self {
        Self {
            key: response.key.clone(),
            pipeline_id: response.pipeline_id.clone(),
            provider_id: response
                .provider_id
                .as_deref()
                .and_then(|id| id.parse().ok()),
            run_number: response
                .run_number
                .as_deref()
                .and_then(|run| run.parse().ok()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeUiAction {
    ManagePins,
    ManageViews,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NativeIntent {
    #[serde(rename_all = "camelCase")]
    Notification {
        action: String,
        #[serde(flatten)]
        target: NotificationTarget,
    },
    #[serde(rename_all = "camelCase")]
    Link {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        run_number: Option<u64>,
    },
    Ui {
        action: NativeUiAction,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingNativeIntent {
    pub id: u64,
    pub intent: NativeIntent,
}

#[derive(Default)]
struct PendingIntentState {
    last_id: u64,
    pending: Option<PendingNativeIntent>,
}

#[derive(Default)]
pub struct PendingIntent(Mutex<PendingIntentState>);

impl PendingIntent {
    fn slot(&self) -> MutexGuard<'_, PendingIntentState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set(&self, intent: NativeIntent) {
        let mut state = self.slot();
        state.last_id += 1;
        let id = state.last_id;
        state.pending = Some(PendingNativeIntent { id, intent });
    }

    pub fn peek(&self) -> Option<PendingNativeIntent> {
        self.slot().pending.clone()
    }

    pub fn ack(&self, id: u64) {
        let mut state = self.slot();
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            state.pending = None;
        }
    }
}

pub fn publish<R: Runtime>(app: &AppHandle<R>, intent: NativeIntent) {
    app.state::<PendingIntent>().set(intent);
    if let Err(e) = app.emit(INTENT_EVENT, ()) {
        tracing::warn!(error = %e, "Couldn't tell the frontend about a pending intent");
    }
}

fn is_route_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'~' | b'-' => index += 1,
            b'%' if bytes
                .get(index + 1..index + 3)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit)) =>
            {
                index += 3
            }
            _ => return false,
        }
    }

    !segment.is_empty() && segment != "." && segment != ".."
}

fn is_section(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
}

pub fn parse_deep_link(url: &Url) -> Option<NativeIntent> {
    if url.scheme() != DEEP_LINK_SCHEME {
        return None;
    }

    let host = url.host_str()?.to_ascii_lowercase();
    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|segment| !segment.is_empty()).collect())
        .unwrap_or_default();
    let link = |path: String, run_number| Some(NativeIntent::Link { path, run_number });

    match (host.as_str(), segments.as_slice()) {
        ("pipelines", []) => link("/pipelines".to_string(), None),
        ("pipelines", [id]) if is_route_segment(id) => link(format!("/pipelines/{id}"), None),
        ("pipelines", [id, "runs", run]) if is_route_segment(id) => {
            link(format!("/pipelines/{id}"), Some(run.parse().ok()?))
        }
        ("notifications", []) => link("/notifications".to_string(), None),
        ("providers", []) => link("/providers".to_string(), None),
        ("settings", []) => link("/settings".to_string(), None),
        ("settings", [section]) if is_section(section) => {
            link(format!("/settings/{section}"), None)
        }
        _ => None,
    }
}

#[cfg_attr(not(mobile), allow(dead_code))]
fn handle_opened<R: Runtime>(app: &AppHandle<R>, urls: &[Url]) {
    for url in urls {
        match parse_deep_link(url) {
            Some(intent) => publish(app, intent),
            None => tracing::debug!(scheme = url.scheme(), "Ignoring a link the app can't route"),
        }
    }
}

#[cfg(mobile)]
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: &tauri::RunEvent) {
    if let tauri::RunEvent::Opened { urls } = event {
        handle_opened(app, urls);
    }
}

#[cfg(not(mobile))]
pub fn on_run_event<R: Runtime>(_app: &AppHandle<R>, _event: &tauri::RunEvent) {}

pub fn privacy_cover_for(settings: AppLockSettings) -> PrivacyCover {
    PrivacyCover {
        armed: settings.enabled,
        hold_for_frontend: settings.lock_on_background,
    }
}

pub fn sync_privacy_cover(app: &AppHandle, settings: AppLockSettings) {
    let app = app.clone();
    let cover = privacy_cover_for(settings);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = app.pipedash_native().configure_privacy_cover(cover).await {
            tracing::warn!(error = %e, "Couldn't configure the native privacy cover");
        }
    });
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let notifications = app.clone();
    let taps = app.clone();
    let native = app.pipedash_native();

    native
        .on_notification_response(move |response| {
            publish(
                &notifications,
                NativeIntent::Notification {
                    action: response.action_id.clone(),
                    target: NotificationTarget::from_response(&response),
                },
            )
        })
        .map_err(|e| tauri::Error::Anyhow(anyhow_from(e)))?;
    native
        .on_status_bar_tap(move || {
            if let Err(e) = taps.emit(STATUS_BAR_TAP_EVENT, ()) {
                tracing::warn!(error = %e, "Couldn't report a status bar tap");
            }
        })
        .map_err(|e| tauri::Error::Anyhow(anyhow_from(e)))
}

fn anyhow_from(error: tauri_plugin_pipedash_native::Error) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}

fn native_error(error: tauri_plugin_pipedash_native::Error) -> ErrorResponse {
    ErrorResponse {
        error: error.to_string(),
        details: None,
        code: None,
    }
}

#[tauri::command]
pub async fn native_haptic(app: AppHandle, kind: HapticKind) -> Result<(), ErrorResponse> {
    app.pipedash_native()
        .haptic(kind)
        .await
        .map_err(native_error)
}

#[tauri::command]
pub async fn native_set_appearance(
    app: AppHandle, scheme: Appearance,
) -> Result<(), ErrorResponse> {
    app.pipedash_native()
        .set_appearance(scheme)
        .await
        .map_err(native_error)
}

#[tauri::command]
pub async fn native_accessibility(app: AppHandle) -> Result<Accessibility, ErrorResponse> {
    app.pipedash_native()
        .accessibility()
        .await
        .map_err(native_error)
}

#[tauri::command]
pub async fn native_open_settings(app: AppHandle) -> Result<(), ErrorResponse> {
    app.pipedash_native()
        .open_settings()
        .await
        .map_err(native_error)
}

#[tauri::command]
pub async fn native_share_link(
    app: AppHandle, url: String, source_rect: ShareRect,
) -> Result<bool, ErrorResponse> {
    app.pipedash_native()
        .share_link(&url, source_rect)
        .await
        .map_err(native_error)
}

#[tauri::command]
pub async fn native_set_badge(app: AppHandle, count: u32) -> Result<(), ErrorResponse> {
    app.pipedash_native()
        .set_badge(count)
        .await
        .map_err(native_error)
}

#[tauri::command]
pub async fn native_release_privacy_cover(app: AppHandle) -> Result<(), ErrorResponse> {
    app.pipedash_native()
        .release_privacy_cover()
        .await
        .map_err(native_error)
}

#[tauri::command]
pub fn native_peek_pending_intent(
    pending: State<'_, PendingIntent>,
) -> Option<PendingNativeIntent> {
    pending.peek()
}

#[tauri::command]
pub fn native_ack_pending_intent(pending: State<'_, PendingIntent>, id: u64) {
    pending.ack(id);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn link(raw: &str) -> Option<NativeIntent> {
        parse_deep_link(&Url::parse(raw).unwrap())
    }

    fn to(path: &str, run_number: Option<u64>) -> Option<NativeIntent> {
        Some(NativeIntent::Link {
            path: path.to_string(),
            run_number,
        })
    }

    #[test]
    fn app_pages_open_from_custom_scheme_links() {
        assert_eq!(link("pipedash://pipelines"), to("/pipelines", None));
        assert_eq!(
            link("pipedash://Notifications/"),
            to("/notifications", None)
        );
        assert_eq!(link("pipedash://providers"), to("/providers", None));
        assert_eq!(link("pipedash://settings"), to("/settings", None));
        assert_eq!(
            link("pipedash://settings/app-lock"),
            to("/settings/app-lock", None)
        );
    }

    #[test]
    fn a_pipeline_link_keeps_its_encoded_id_and_an_optional_run() {
        assert_eq!(
            link("pipedash://pipelines/github__1__acme%2Fapp%2Fci.yml"),
            to("/pipelines/github__1__acme%2Fapp%2Fci.yml", None)
        );
        assert_eq!(
            link("pipedash://pipelines/jenkins__2__build/runs/41"),
            to("/pipelines/jenkins__2__build", Some(41))
        );
    }

    #[test]
    fn links_the_app_cannot_route_are_ignored() {
        for raw in [
            "https://pipelines/abc",
            "pipedash://unknown",
            "pipedash://pipelines/a/b",
            "pipedash://pipelines/bad%zz",
            "pipedash://pipelines/abc/runs/not-a-number",
            "pipedash://pipelines/abc/runs/-1",
            "pipedash://settings/Bad_Section",
            "pipedash://settings/a/b",
        ] {
            assert_eq!(link(raw), None, "{raw}");
        }
    }

    #[test]
    fn peeking_does_not_consume_the_pending_intent() {
        let pending = PendingIntent::default();
        assert_eq!(pending.peek(), None);

        let intent = NativeIntent::Link {
            path: "/providers".to_string(),
            run_number: None,
        };
        pending.set(intent.clone());
        let first = pending.peek().unwrap();

        assert_eq!(first.intent, intent);
        assert_eq!(pending.peek(), Some(first));
    }

    #[test]
    fn the_latest_intent_survives_a_stale_acknowledgment() {
        let pending = PendingIntent::default();
        pending.set(NativeIntent::Ui {
            action: NativeUiAction::ManagePins,
        });
        let first = pending.peek().unwrap();
        let latest_intent = NativeIntent::Notification {
            action: "tap".to_string(),
            target: NotificationTarget {
                key: Some("pipeline:p1".to_string()),
                ..Default::default()
            },
        };
        pending.set(latest_intent.clone());
        let latest = pending.peek().unwrap();

        assert!(latest.id > first.id);
        assert_eq!(latest.intent, latest_intent);
        pending.ack(first.id);
        assert_eq!(pending.peek(), Some(latest.clone()));
        pending.ack(latest.id);
        assert_eq!(pending.peek(), None);
    }

    #[test]
    fn pending_intent_ids_increase_after_acknowledgment() {
        let pending = PendingIntent::default();
        let intent = NativeIntent::Ui {
            action: NativeUiAction::ManageViews,
        };
        pending.set(intent.clone());
        let first = pending.peek().unwrap();
        pending.ack(first.id);
        pending.set(intent);

        assert!(pending.peek().unwrap().id > first.id);
    }

    #[test]
    fn ui_intents_serialize_camel_case_actions_in_pending_envelopes() {
        for (action, serialized) in [
            (NativeUiAction::ManagePins, "managePins"),
            (NativeUiAction::ManageViews, "manageViews"),
        ] {
            let pending = PendingNativeIntent {
                id: 4,
                intent: NativeIntent::Ui { action },
            };

            assert_eq!(
                serde_json::to_value(pending).unwrap(),
                json!({ "id": 4, "intent": { "type": "ui", "action": serialized } })
            );
        }
    }

    #[test]
    fn intents_serialize_flat_with_a_type_tag() {
        let notification = NativeIntent::Notification {
            action: "open-run".to_string(),
            target: NotificationTarget {
                key: Some("pipeline:p1".to_string()),
                pipeline_id: Some("p1".to_string()),
                provider_id: Some(3),
                run_number: Some(12),
            },
        };
        assert_eq!(
            serde_json::to_value(notification).unwrap(),
            json!({
                "type": "notification",
                "action": "open-run",
                "key": "pipeline:p1",
                "pipelineId": "p1",
                "providerId": 3,
                "runNumber": 12,
            })
        );
        assert_eq!(
            serde_json::to_value(to("/pipelines", None).unwrap()).unwrap(),
            json!({ "type": "link", "path": "/pipelines" })
        );
    }

    #[test]
    fn a_local_notification_target_survives_the_extra_round_trip() {
        let target = NotificationTarget {
            key: Some("pipeline:p1".to_string()),
            pipeline_id: Some("p1".to_string()),
            provider_id: Some(3),
            run_number: None,
        };
        let extra: HashMap<String, Value> = target
            .extra()
            .into_iter()
            .map(|(name, value)| (name.to_string(), Value::String(value)))
            .collect();

        assert_eq!(extra.len(), 3);
        assert_eq!(NotificationTarget::from_extra(&extra), target);
    }

    #[test]
    fn a_native_response_parses_numbers_and_drops_bad_ones() {
        let response = NotificationResponse {
            action_id: "tap".to_string(),
            key: Some("k".to_string()),
            pipeline_id: Some("p".to_string()),
            provider_id: Some("4".to_string()),
            run_number: Some("seven".to_string()),
        };

        assert_eq!(
            NotificationTarget::from_response(&response),
            NotificationTarget {
                key: Some("k".to_string()),
                pipeline_id: Some("p".to_string()),
                provider_id: Some(4),
                run_number: None,
            }
        );
    }

    #[test]
    fn the_privacy_cover_follows_the_app_lock_settings() {
        let cover = privacy_cover_for(AppLockSettings {
            enabled: true,
            lock_on_background: false,
        });
        assert!(cover.armed);
        assert!(!cover.hold_for_frontend);
        assert!(!privacy_cover_for(AppLockSettings::default()).armed);
    }
}
