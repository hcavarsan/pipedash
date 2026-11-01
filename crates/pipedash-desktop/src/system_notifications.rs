use tauri::{
    AppHandle,
    Runtime,
};
#[cfg(not(target_os = "ios"))]
use tauri_plugin_notification::ActionPerformed;
use tauri_plugin_notification::NotificationExt;

#[cfg(not(target_os = "ios"))]
use crate::native::{
    publish,
    NativeIntent,
};
use crate::{
    commands::ErrorResponse,
    native::{
        NotificationTarget,
        FAILURE_CATEGORY,
    },
};

#[cfg(desktop)]
const MAIN_WINDOW: &str = "main";
#[cfg(not(target_os = "ios"))]
const TAP_ACTION: &str = "tap";

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    #[cfg(not(target_os = "ios"))]
    {
        let handle = app.clone();
        app.notification()
            .on_action(move |performed| on_action(&handle, performed))
            .map_err(|e| tauri::Error::Anyhow(e.into()))?;
    }

    crate::native::register(app)
}

#[cfg(not(target_os = "ios"))]
fn clicked_target(performed: &ActionPerformed) -> Option<NotificationTarget> {
    if performed.action_id() != TAP_ACTION {
        return None;
    }
    let target = NotificationTarget::from_extra(performed.notification()?.extra());

    (target != NotificationTarget::default()).then_some(target)
}

#[cfg(desktop)]
pub(crate) fn focus_main_window<R: Runtime>(app: &AppHandle<R>) {
    use tauri::Manager;

    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        if let Err(error) = window.unminimize() {
            tracing::warn!(%error, "Couldn't restore the Pipedash window");
        }
        if let Err(error) = window.show() {
            tracing::warn!(%error, "Couldn't show the Pipedash window");
        }
        if let Err(e) = window.set_focus() {
            tracing::warn!(error = %e, "Couldn't bring the Pipedash window to the front");
        }
    }
}

#[cfg(not(target_os = "ios"))]
fn on_action<R: Runtime>(app: &AppHandle<R>, performed: &ActionPerformed) {
    let Some(target) = clicked_target(performed) else {
        return;
    };

    #[cfg(desktop)]
    focus_main_window(app);
    publish(
        app,
        NativeIntent::Notification {
            action: TAP_ACTION.to_string(),
            target,
        },
    );
}

fn category_for(target: &NotificationTarget) -> Option<&'static str> {
    target.pipeline_id.as_ref().map(|_| FAILURE_CATEGORY)
}

#[tauri::command]
pub fn show_system_notification<R: Runtime>(
    app: AppHandle<R>, title: String, body: String, target: Option<NotificationTarget>,
) -> Result<(), ErrorResponse> {
    let mut builder = app.notification().builder().title(title).body(body);
    if let Some(target) = target {
        if let Some(category) = category_for(&target) {
            builder = builder.action_type_id(category);
        }
        for (name, value) in target.extra() {
            builder = builder.extra(name, value);
        }
    }

    builder.show().map_err(|e| ErrorResponse {
        error: format!("Couldn't show the notification: {e}"),
        details: None,
        code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_notifications_about_a_pipeline_get_the_open_run_category() {
        let pipeline = NotificationTarget {
            key: Some("pipeline:p1".to_string()),
            pipeline_id: Some("p1".to_string()),
            ..Default::default()
        };
        let provider = NotificationTarget {
            key: Some("provider:3".to_string()),
            ..Default::default()
        };

        assert_eq!(category_for(&pipeline), Some(FAILURE_CATEGORY));
        assert_eq!(category_for(&provider), None);
    }

    #[cfg(desktop)]
    mod clicks {
        use serde_json::{
            json,
            Value,
        };

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
                json!({
                    "id": 7,
                    "title": "build failed",
                    "extra": { "key": "pipeline:p1", "pipelineId": "p1", "providerId": "3" }
                }),
            );

            assert_eq!(
                clicked_target(&tapped),
                Some(NotificationTarget {
                    key: Some("pipeline:p1".to_string()),
                    pipeline_id: Some("p1".to_string()),
                    provider_id: Some(3),
                    run_number: None,
                })
            );
        }

        #[test]
        fn other_actions_and_notifications_without_a_target_report_nothing() {
            let with_target = json!({ "id": 7, "extra": { "key": "status-page#3" } });

            assert_eq!(clicked_target(&performed("dismiss", with_target)), None);
            assert_eq!(clicked_target(&performed("tap", json!({ "id": 7 }))), None);
            assert_eq!(clicked_target(&performed("tap", Value::Null)), None);
        }
    }
}
