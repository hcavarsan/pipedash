use serde_json::Value;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
};
use tauri_plugin_notification::{
    ActionPerformed,
    NotificationExt,
};

use crate::commands::ErrorResponse;

pub const CLICKED_EVENT: &str = "system-notification-clicked";
const MAIN_WINDOW: &str = "main";
const TAP_ACTION: &str = "tap";
const TARGET_KEY: &str = "target";

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let handle = app.clone();
    app.notification()
        .on_action(move |performed| on_action(&handle, performed))
        .map_err(|e| tauri::Error::Anyhow(e.into()))
}

fn clicked_target(performed: &ActionPerformed) -> Option<&str> {
    if performed.action_id() != TAP_ACTION {
        return None;
    }
    performed
        .notification()?
        .extra()
        .get(TARGET_KEY)
        .and_then(Value::as_str)
}

fn on_action<R: Runtime>(app: &AppHandle<R>, performed: &ActionPerformed) {
    let Some(target) = clicked_target(performed) else {
        return;
    };

    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        if let Err(e) = window.set_focus() {
            tracing::warn!(error = %e, "Couldn't focus the window after a notification click");
        }
    }

    if let Err(e) = app.emit(CLICKED_EVENT, target) {
        tracing::warn!(error = %e, "Couldn't report a notification click");
    }
}

#[tauri::command]
pub fn show_system_notification<R: Runtime>(
    app: AppHandle<R>, title: String, body: String, target: Option<String>,
) -> Result<(), ErrorResponse> {
    let mut builder = app.notification().builder().title(title).body(body);
    if let Some(target) = target {
        builder = builder.extra(TARGET_KEY, target);
    }

    builder.show().map_err(|e| ErrorResponse {
        error: format!("Couldn't show the notification: {e}"),
        details: None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn performed(action_id: &str, notification: Value) -> ActionPerformed {
        serde_json::from_value(json!({
            "actionId": action_id,
            "inputValue": null,
            "notification": notification,
        }))
        .unwrap()
    }

    #[test]
    fn a_click_on_a_notification_reports_its_target() {
        let tapped = performed(
            "tap",
            json!({ "id": 7, "title": "GitHub is down", "extra": { "target": "status-page#3" } }),
        );

        assert_eq!(clicked_target(&tapped), Some("status-page#3"));
    }

    #[test]
    fn other_actions_and_notifications_without_a_target_report_nothing() {
        let with_target = json!({ "id": 7, "extra": { "target": "status-page#3" } });

        assert_eq!(clicked_target(&performed("dismiss", with_target)), None);
        assert_eq!(clicked_target(&performed("tap", json!({ "id": 7 }))), None);
        assert_eq!(clicked_target(&performed("tap", Value::Null)), None);
    }
}
