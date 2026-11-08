use axum::http::StatusCode;
use chrono::{
    DateTime,
    Utc,
};
use pipedash_core::domain::pairing::{
    display_pairing_code,
    pairing_link,
};
use pipedash_core::domain::ServerUrl;
use qrcode::render::svg;
use qrcode::QrCode;
use serde::Serialize;

use crate::error::{
    ApiError,
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct PairingResponse {
    pub code: String,
    pub link: String,
    pub server_url: String,
    pub expires_at: DateTime<Utc>,
    pub loopback: bool,
    pub qr_svg: String,
}

pub fn create_pairing(state: &AppState, server_url: &str) -> ApiResult<PairingResponse> {
    let server = ServerUrl::parse(server_url).map_err(|reason| {
        AppError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::new(
                "INVALID_SERVER_URL",
                format!("Invalid server address: {reason}"),
            ),
        )
    })?;
    let (code, expires_at) = state.pairing_codes.issue()?;
    let link = pairing_link(&server.url, &code);
    let qr_svg = QrCode::new(link.as_bytes())
        .map_err(|error| AppError::internal(error.to_string()))?
        .render::<svg::Color>()
        .min_dimensions(240, 240)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(PairingResponse {
        code: display_pairing_code(&code),
        loopback: server.is_loopback(),
        server_url: server.url,
        link,
        expires_at,
        qr_svg,
    })
}
