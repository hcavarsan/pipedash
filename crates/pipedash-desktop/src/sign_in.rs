use std::collections::HashMap;
use std::sync::Arc;

use pipedash_core::application::ProviderService;
use pipedash_core::{
    SignInStart,
    SignInStatus,
};
use tauri::{
    AppHandle,
    State,
};

use crate::commands::ErrorResponse;

async fn provider_service(
    maybe_core: &crate::MaybeCoreContext,
) -> Result<Arc<ProviderService>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(Arc::clone(&core.provider_service))
}

#[tauri::command]
pub async fn start_sign_in(
    app: AppHandle, maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>,
) -> Result<SignInStart, ErrorResponse> {
    let service = provider_service(&maybe_core).await?;
    let start = service.start_sign_in(&provider_type, config).await?;
    open_sign_in(&app, service, &start);
    Ok(start)
}

#[tauri::command]
pub async fn start_provider_sign_in(
    app: AppHandle, maybe_core: State<'_, crate::MaybeCoreContext>, id: i64,
) -> Result<SignInStart, ErrorResponse> {
    let service = provider_service(&maybe_core).await?;
    let start = service.start_provider_sign_in(id).await?;
    open_sign_in(&app, service, &start);
    Ok(start)
}

#[tauri::command]
pub async fn reconnect_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64, flow_id: String,
) -> Result<(), ErrorResponse> {
    Ok(provider_service(&maybe_core)
        .await?
        .reconnect_provider(id, &flow_id)
        .await?)
}

#[tauri::command]
pub async fn sign_in_status(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: String,
) -> Result<SignInStatus, ErrorResponse> {
    let service = provider_service(&maybe_core).await?;
    Ok(service.sign_in_status(&id).await?)
}

#[tauri::command]
pub async fn complete_sign_in(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: String, redirect_url: String,
) -> Result<SignInStatus, ErrorResponse> {
    let service = provider_service(&maybe_core).await?;
    Ok(service.complete_sign_in(&id, &redirect_url).await?)
}

#[tauri::command]
pub async fn cancel_sign_in(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: String,
) -> Result<(), ErrorResponse> {
    provider_service(&maybe_core)
        .await?
        .cancel_sign_in(&id)
        .await;
    Ok(())
}

#[cfg(desktop)]
fn open_sign_in(app: &AppHandle, service: Arc<ProviderService>, start: &SignInStart) {
    use tauri_plugin_opener::OpenerExt;

    if let Err(error) = app.opener().open_url(&start.authorize_url, None::<&str>) {
        tracing::warn!(%error, "Couldn't open the sign-in page in the browser");
    }

    let app = app.clone();
    let id = start.id.clone();
    tauri::async_runtime::spawn(async move {
        if matches!(
            service.wait_for_sign_in(&id).await,
            SignInStatus::Completed { .. }
        ) {
            if let Err(error) = crate::desktop::show_main_window(&app) {
                tracing::warn!(%error, "Couldn't show Pipedash after sign-in");
            }
        }
    });
}

#[cfg(mobile)]
fn open_sign_in(app: &AppHandle, service: Arc<ProviderService>, start: &SignInStart) {
    use tauri_plugin_pipedash_native::PipedashNativeExt;

    let presenter = app.clone();
    let url = start.authorize_url.clone();
    let id = start.id.clone();
    let cancelled_by = Arc::clone(&service);
    tauri::async_runtime::spawn(async move {
        match presenter.pipedash_native().open_auth_session(&url).await {
            Ok(true) => cancelled_by.cancel_sign_in(&id).await,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "Couldn't show the sign-in sheet"),
        }
    });

    let closer = app.clone();
    let id = start.id.clone();
    tauri::async_runtime::spawn(async move {
        service.wait_for_sign_in(&id).await;
        if let Err(error) = closer.pipedash_native().close_auth_session().await {
            tracing::warn!(%error, "Couldn't close the sign-in sheet");
        }
    });
}
