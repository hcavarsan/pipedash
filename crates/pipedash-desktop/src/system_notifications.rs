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
    error::ErrorResponse,
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
mod tests;
