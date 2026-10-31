use tauri::{
    AppHandle,
    Runtime,
};
#[cfg(not(target_os = "ios"))]
use tauri_plugin_notification::ActionPerformed;
use tauri_plugin_notification::NotificationExt;
#[cfg(desktop)]
use tauri_plugin_notification::{
    Action,
    ActionType,
};

#[cfg(mobile)]
use crate::native::FAILURE_CATEGORY;
#[cfg(not(target_os = "ios"))]
use crate::native::{
    publish,
    NativeIntent,
};
use crate::{
    commands::ErrorResponse,
    native::NotificationTarget,
};

#[cfg(desktop)]
const OPEN_CATEGORY: &str = "pipedash-open";
#[cfg(desktop)]
const OPEN_ACTION: &str = "open";
#[cfg(not(target_os = "ios"))]
const TAP_ACTION: &str = "tap";

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    #[cfg(desktop)]
    app.notification()
        .register_action_types(vec![ActionType::builder(OPEN_CATEGORY)
            .actions(vec![Action::builder(OPEN_ACTION, "Open Pipedash").build()])
            .build()])
        .map_err(|e| tauri::Error::Anyhow(e.into()))?;

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
    match performed.action_id() {
        TAP_ACTION => {}
        #[cfg(desktop)]
        OPEN_ACTION => {}
        _ => return None,
    }
    let target = NotificationTarget::from_extra(performed.notification()?.extra());

    (target != NotificationTarget::default()).then_some(target)
}

#[cfg(not(target_os = "ios"))]
fn on_action<R: Runtime>(app: &AppHandle<R>, performed: &ActionPerformed) {
    let Some(target) = clicked_target(performed) else {
        return;
    };

    #[cfg(desktop)]
    if let Err(e) = crate::desktop::show_main_window(app) {
        tracing::warn!(error = %e, "Couldn't bring the Pipedash window to the front");
    }
    publish(
        app,
        NativeIntent::Notification {
            action: performed.action_id().to_string(),
            target,
        },
    );
}

#[cfg(mobile)]
fn category_for(target: &NotificationTarget) -> Option<&'static str> {
    target.pipeline_id.as_ref().map(|_| FAILURE_CATEGORY)
}

pub(crate) fn send<R: Runtime>(
    app: &AppHandle<R>, title: &str, body: &str, target: Option<&NotificationTarget>,
) -> Result<(), ErrorResponse> {
    let mut builder = app.notification().builder().title(title).body(body);
    #[cfg(desktop)]
    {
        builder = builder.action_type_id(OPEN_CATEGORY);
    }
    if let Some(target) = target {
        #[cfg(mobile)]
        if let Some(category) = category_for(target) {
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

#[tauri::command]
pub fn show_system_notification<R: Runtime>(
    app: AppHandle<R>, title: String, body: String, target: Option<NotificationTarget>,
) -> Result<(), ErrorResponse> {
    send(&app, &title, &body, target.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(mobile)]
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

    #[cfg(not(target_os = "ios"))]
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
                    "extra": {
                        "key": "pipeline:p1",
                        "pipelineId": "p1",
                        "providerId": "3",
                        "runNumber": "42"
                    }
                }),
            );

            assert_eq!(
                clicked_target(&tapped),
                Some(NotificationTarget {
                    key: Some("pipeline:p1".to_string()),
                    pipeline_id: Some("p1".to_string()),
                    provider_id: Some(3),
                    run_number: Some(42),
                })
            );
        }

        #[cfg(desktop)]
        #[test]
        fn desktop_actions_accept_provider_and_test_notification_targets() {
            for (extra, target) in [
                (
                    json!({ "key": "provider:3", "providerId": "3" }),
                    NotificationTarget {
                        key: Some("provider:3".to_string()),
                        provider_id: Some(3),
                        ..Default::default()
                    },
                ),
                (
                    json!({ "key": "desktop-test" }),
                    NotificationTarget {
                        key: Some("desktop-test".to_string()),
                        ..Default::default()
                    },
                ),
            ] {
                for action in ["tap", "open"] {
                    let clicked = performed(action, json!({ "id": 7, "extra": extra.clone() }));

                    assert_eq!(clicked_target(&clicked), Some(target.clone()));
                }
            }
        }

        #[cfg(target_os = "android")]
        #[test]
        fn android_does_not_accept_the_desktop_open_action() {
            let opened = performed(
                "open",
                json!({ "id": 7, "extra": { "key": "pipeline:p1" } }),
            );

            assert_eq!(clicked_target(&opened), None);
        }

        #[test]
        fn other_actions_and_notifications_without_a_target_report_nothing() {
            let with_target = json!({ "id": 7, "extra": { "key": "status-page#3" } });

            for action in ["dismiss", "open-run", "retry", "cancel"] {
                assert_eq!(
                    clicked_target(&performed(action, with_target.clone())),
                    None
                );
            }
            assert_eq!(clicked_target(&performed("tap", json!({ "id": 7 }))), None);
            assert_eq!(clicked_target(&performed("tap", Value::Null)), None);
        }
    }
}
