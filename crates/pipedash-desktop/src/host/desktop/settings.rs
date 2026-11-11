use std::io::ErrorKind;
use std::net::{
    IpAddr,
    UdpSocket,
};
use std::path::Path;

use serde::{
    Deserialize,
    Serialize,
};

use crate::host::SharingErrorCode;

pub(super) const SETTINGS_FILE: &str = "host.json";
pub(super) const TMP_SETTINGS_FILE: &str = "host.json.tmp";
pub(super) const NO_NETWORK: &str = "Connect this computer to a network to share Pipedash.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct HostSettings {
    pub(super) enabled: bool,
    pub(super) port: Option<u16>,
    pub(super) relay_checked: bool,
}

pub(super) struct SharingFailure {
    pub(super) message: String,
    pub(super) code: Option<SharingErrorCode>,
}

pub(super) fn read_settings(dir: &Path) -> HostSettings {
    match std::fs::read(dir.join(SETTINGS_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Ignoring unreadable sharing settings");
            HostSettings::default()
        }),
        Err(_) => HostSettings::default(),
    }
}

pub(super) fn write_settings(dir: &Path, settings: HostSettings) -> std::io::Result<()> {
    let tmp = dir.join(TMP_SETTINGS_FILE);
    std::fs::write(&tmp, serde_json::to_vec(&settings)?)?;
    std::fs::rename(&tmp, dir.join(SETTINGS_FILE))
}

pub(super) fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();

    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

pub(super) fn failure(
    message: impl Into<String>, code: Option<SharingErrorCode>,
) -> SharingFailure {
    SharingFailure {
        message: message.into(),
        code,
    }
}

pub(super) fn bind_failure(saved_port: Option<u16>, e: &std::io::Error) -> SharingFailure {
    match saved_port {
        Some(port) if e.kind() == ErrorKind::AddrInUse => failure(
            format!("Port {port} is in use by another app."),
            Some(SharingErrorCode::PortInUse),
        ),
        _ => failure(format!("Couldn't start sharing: {e}"), None),
    }
}
