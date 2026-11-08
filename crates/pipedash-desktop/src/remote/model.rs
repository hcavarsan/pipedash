use pipedash_core::domain::ServerUrl;
use serde::Serialize;

use super::store::{
    HostKind,
    SavedConnection,
    SavedMode,
};
use crate::error::ErrorResponse;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("Invalid server address: {0}")]
    InvalidUrl(&'static str),
    #[error("Enter the 8-character pairing code.")]
    CodeRequired,
    #[error("Pairing codes have 8 letters and numbers, like ABCD-EFGH.")]
    InvalidCode,
    #[error("Use https:// for servers outside your private network.")]
    PublicHttp,
    #[error("This server uses an unencrypted connection.")]
    PrivateHttpDisabled,
    #[error("This device connects over HTTP. Disconnect from the server before turning this off.")]
    InsecureInUse,
    #[error("{message}")]
    Rejected { code: String, message: String },
    #[error("Couldn't reach {0}. Check the address and that this device can reach it.")]
    Unreachable(String),
    #[error("The server took too long to answer.")]
    Timeout,
    #[error("This device isn't connected to a server.")]
    NotConnected,
    #[error("Disconnect from the server first.")]
    Connected,
    #[error("Invalid request.")]
    InvalidRequest,
    #[error("Couldn't access the system keychain.")]
    Keychain,
    #[error("Couldn't save the connection settings.")]
    Storage,
    #[error("Couldn't start Pipedash on this device: {0}")]
    Local(String),
    #[error("Couldn't download the file (HTTP {0}).")]
    Download(u16),
}

impl RemoteError {
    fn code(&self) -> &str {
        match self {
            Self::InvalidUrl(_) => "INVALID_URL",
            Self::CodeRequired => "CODE_REQUIRED",
            Self::InvalidCode => "INVALID_CODE",
            Self::PublicHttp => "PUBLIC_HTTP",
            Self::PrivateHttpDisabled => "PRIVATE_HTTP_DISABLED",
            Self::InsecureInUse => "INSECURE_IN_USE",
            Self::Rejected { code, .. } => code,
            Self::Unreachable(_) => "UNREACHABLE",
            Self::Timeout => "TIMEOUT",
            Self::NotConnected => "NOT_CONNECTED",
            Self::Connected => "CONNECTED",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::Keychain => "KEYCHAIN",
            Self::Storage => "STORAGE",
            Self::Local(_) => "LOCAL_START",
            Self::Download(_) => "DOWNLOAD_FAILED",
        }
    }
}

impl From<RemoteError> for ErrorResponse {
    fn from(error: RemoteError) -> Self {
        Self {
            error: error.to_string(),
            details: None,
            code: Some(error.code().to_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStatus {
    Connecting,
    Connected,
    Reconnecting,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionMode {
    Unset,
    Standalone,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    mode: ConnectionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_kind: Option<HostKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<RemoteStatus>,
    allow_private_http: bool,
}

pub(super) fn display_host(server_url: &str, host_name: Option<&str>) -> String {
    host_name.map(str::to_owned).unwrap_or_else(|| {
        ServerUrl::parse(server_url).map_or_else(|_| server_url.to_owned(), |parsed| parsed.host)
    })
}

impl ConnectionInfo {
    pub(super) fn new(saved: &SavedConnection, status: RemoteStatus) -> Self {
        let mut info = Self {
            mode: ConnectionMode::Unset,
            server_url: None,
            host: None,
            host_kind: None,
            device_id: None,
            status: None,
            allow_private_http: saved.allow_private_http,
        };
        match &saved.mode {
            SavedMode::Unset => {}
            SavedMode::Standalone => info.mode = ConnectionMode::Standalone,
            SavedMode::Client {
                server_url,
                device_id,
                host_kind,
                host_name,
            } => {
                info.mode = ConnectionMode::Client;
                info.host = Some(display_host(server_url, host_name.as_deref()));
                info.server_url = Some(server_url.clone());
                info.host_kind = Some(*host_kind);
                info.device_id = Some(device_id.clone());
                info.status = Some(status);
            }
        }

        info
    }
}

#[derive(Debug, Serialize)]
pub struct RemoteResponse {
    pub(super) status: u16,
    pub(super) body: String,
}
