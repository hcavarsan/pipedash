use tauri::AppHandle;
#[cfg(mobile)]
use tauri_plugin_biometric::{
    AuthOptions,
    BiometricExt,
    BiometryType,
};

#[cfg(mobile)]
use super::{
    classify_rejection,
    Biometry,
};
use super::{
    AuthOutcome,
    BiometryInfo,
};

#[cfg(mobile)]
pub(super) async fn info(app: &AppHandle) -> BiometryInfo {
    let app = app.clone();
    match tauri::async_runtime::spawn_blocking(move || app.biometric().status()).await {
        Ok(Ok(status)) => BiometryInfo {
            biometry: match status.biometry_type {
                BiometryType::None => Biometry::None,
                BiometryType::TouchID => Biometry::TouchId,
                BiometryType::FaceID => Biometry::FaceId,
            },
            available: status.is_available,
        },
        Ok(Err(e)) => {
            tracing::warn!("Couldn't read the biometric status: {}", e);
            BiometryInfo::NONE
        }
        Err(e) => {
            tracing::warn!("Biometric status task failed: {}", e);
            BiometryInfo::NONE
        }
    }
}

#[cfg(mobile)]
pub(super) async fn authenticate(app: &AppHandle, reason: &str) -> AuthOutcome {
    let app = app.clone();
    let reason = reason.to_string();
    let options = AuthOptions {
        allow_device_credential: true,
        ..Default::default()
    };

    match tauri::async_runtime::spawn_blocking(move || {
        app.biometric().authenticate(reason, options)
    })
    .await
    {
        Ok(Ok(())) => AuthOutcome::Authenticated,
        Ok(Err(tauri_plugin_biometric::Error::PluginInvoke(
            tauri::plugin::mobile::PluginInvokeError::InvokeRejected(rejection),
        ))) => classify_rejection(
            rejection.code.as_deref(),
            rejection.message.as_deref().unwrap_or_default(),
        ),
        Ok(Err(e)) => AuthOutcome::Failed(e.to_string()),
        Err(e) => AuthOutcome::Failed(e.to_string()),
    }
}

#[cfg(not(mobile))]
pub(super) async fn info(_app: &AppHandle) -> BiometryInfo {
    BiometryInfo::NONE
}

#[cfg(not(mobile))]
pub(super) async fn authenticate(_app: &AppHandle, _reason: &str) -> AuthOutcome {
    AuthOutcome::Failed("App lock is not supported on this platform".to_string())
}
