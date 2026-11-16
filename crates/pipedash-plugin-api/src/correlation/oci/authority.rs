use super::is_loopback_host;

pub fn registry_authority(host: &str) -> String {
    let host = host.trim().to_ascii_lowercase();
    let host = host.as_str();
    let scheme = if is_loopback_host(host) {
        "http"
    } else {
        "https"
    };
    strip_default_port(host, scheme)
}

pub(super) fn strip_default_port(authority: &str, scheme: &str) -> String {
    let authority = authority.to_ascii_lowercase();
    let default = if scheme == "https" { ":443" } else { ":80" };
    match authority.strip_suffix(default) {
        Some(bare) => bare.to_string(),
        None => authority,
    }
}
