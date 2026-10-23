use std::net::{
    IpAddr,
    Ipv4Addr,
    Ipv6Addr,
};
use std::sync::{
    Arc,
    OnceLock,
};
use std::time::Duration;

use dashmap::DashMap;
use pipedash_plugin_api::ClientOptions;
use reqwest::{
    redirect,
    Client,
    ClientBuilder,
    Url,
};

use crate::domain::{
    DomainError,
    DomainResult,
};

const USER_AGENT: &str = concat!("pipedash/", env!("CARGO_PKG_VERSION"));
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_MAX_REDIRECTS: usize = 3;

type ClientKey = (Option<String>, bool);

pub struct HttpClientManager {
    default_client: Arc<Client>,
    clients: DashMap<ClientKey, Arc<Client>>,
    status_client: OnceLock<Arc<Client>>,
}

impl HttpClientManager {
    pub fn new() -> DomainResult<Self> {
        let default_client = Self::build_client(false)?;
        Ok(Self {
            default_client: Arc::new(default_client),
            clients: DashMap::new(),
            status_client: OnceLock::new(),
        })
    }

    pub fn client_for(&self, options: &ClientOptions) -> DomainResult<Arc<Client>> {
        if *options == ClientOptions::default() {
            return Ok(Arc::clone(&self.default_client));
        }

        let key = (
            options.origin.as_ref().map(normalize_origin),
            options.accept_invalid_certs,
        );
        let entry = self.clients.entry(key).or_try_insert_with(|| {
            Self::build_client(options.accept_invalid_certs).map(Arc::new)
        })?;
        Ok(Arc::clone(entry.value()))
    }

    pub fn status_client(&self) -> DomainResult<Arc<Client>> {
        if let Some(client) = self.status_client.get() {
            return Ok(Arc::clone(client));
        }
        let client = Arc::new(Self::build_status_client()?);
        Ok(Arc::clone(self.status_client.get_or_init(|| client)))
    }

    fn base_builder() -> ClientBuilder {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Client::builder()
            .tls_backend_rustls()
            .user_agent(USER_AGENT)
    }

    fn build_client(accept_invalid_certs: bool) -> DomainResult<Client> {
        let pool_size = std::env::var("PIPEDASH_HTTP_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        Self::base_builder()
            .tls_danger_accept_invalid_certs(accept_invalid_certs)
            .pool_max_idle_per_host(pool_size)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| DomainError::InternalError(format!("Failed to create HTTP client: {}", e)))
    }

    fn build_status_client() -> DomainResult<Client> {
        let policy = redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > STATUS_MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if is_safe_redirect_target(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirect target refused")
            }
        });

        Self::base_builder()
            .redirect(policy)
            .timeout(STATUS_TIMEOUT)
            .connect_timeout(STATUS_TIMEOUT)
            .build()
            .map_err(|e| {
                DomainError::InternalError(format!("Failed to create status HTTP client: {}", e))
            })
    }
}

fn normalize_origin(url: &Url) -> String {
    format!(
        "{}://{}:{}",
        url.scheme().to_ascii_lowercase(),
        url.host_str().unwrap_or_default().to_ascii_lowercase(),
        url.port_or_known_default().unwrap_or_default()
    )
}

fn is_safe_redirect_target(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    match host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
    {
        Ok(ip) => !is_internal_ip(ip),
        Err(_) => true,
    }
}

fn is_internal_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_internal_v4(v4),
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or_else(|| is_internal_v6(v6), is_internal_v4),
    }
}

fn is_internal_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
}

fn is_internal_v6(ip: Ipv6Addr) -> bool {
    ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local() || ip.is_unspecified()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(origin: &str, accept_invalid_certs: bool) -> ClientOptions {
        ClientOptions {
            origin: Some(Url::parse(origin).unwrap()),
            accept_invalid_certs,
        }
    }

    #[test]
    fn default_options_share_the_default_client() {
        let manager = HttpClientManager::new().unwrap();
        let a = manager.client_for(&ClientOptions::default()).unwrap();
        let b = manager.client_for(&ClientOptions::default()).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn equivalent_origins_share_one_client() {
        let manager = HttpClientManager::new().unwrap();
        let a = manager.client_for(&options("https://h", false)).unwrap();
        let b = manager
            .client_for(&options("https://H:443/", false))
            .unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert!(!Arc::ptr_eq(&a, &manager.default_client));
    }

    #[test]
    fn insecure_and_secure_clients_differ_for_same_origin() {
        let manager = HttpClientManager::new().unwrap();
        let secure = manager
            .client_for(&options("https://argo.local", false))
            .unwrap();
        let insecure = manager
            .client_for(&options("https://argo.local", true))
            .unwrap();
        assert!(!Arc::ptr_eq(&secure, &insecure));
    }

    #[test]
    fn different_origins_get_different_clients() {
        let manager = HttpClientManager::new().unwrap();
        let a = manager
            .client_for(&options("https://a.example", false))
            .unwrap();
        let b = manager
            .client_for(&options("https://a.example:8443", false))
            .unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn status_client_is_cached_and_separate() {
        let manager = HttpClientManager::new().unwrap();
        let a = manager.status_client().unwrap();
        let b = manager.status_client().unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        let insecure = manager
            .client_for(&options("https://a.example", true))
            .unwrap();
        assert!(!Arc::ptr_eq(&a, &insecure));
        assert!(!Arc::ptr_eq(&a, &manager.default_client));
    }

    #[test]
    fn redirect_targets_must_be_public_https() {
        for allowed in [
            "https://www.githubstatus.com/api/v2/summary.json",
            "https://8.8.8.8/",
        ] {
            assert!(
                is_safe_redirect_target(&Url::parse(allowed).unwrap()),
                "{allowed}"
            );
        }
        for refused in [
            "http://www.githubstatus.com/",
            "http://127.0.0.1:8765/",
            "https://127.0.0.1/",
            "https://10.1.2.3/",
            "https://192.168.0.1/",
            "https://169.254.169.254/",
            "https://0.0.0.0/",
            "https://[::1]/",
            "https://[fe80::1]/",
            "https://[fd00::1]/",
            "https://[::ffff:127.0.0.1]/",
        ] {
            assert!(
                !is_safe_redirect_target(&Url::parse(refused).unwrap()),
                "{refused}"
            );
        }
    }
}
