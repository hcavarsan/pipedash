use std::net::{
    IpAddr,
    Ipv4Addr,
    Ipv6Addr,
};
use std::sync::{
    Arc,
    OnceLock,
    PoisonError,
    RwLock,
};
use std::time::Duration;

use dashmap::DashMap;
use pipedash_plugin_api::ClientOptions;
use reqwest::{
    redirect,
    Client,
    ClientBuilder,
    NoProxy,
    Proxy,
    Url,
};

use crate::domain::{
    DomainError,
    DomainResult,
};

const USER_AGENT: &str = concat!("pipedash/", env!("CARGO_PKG_VERSION"));
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_MAX_REDIRECTS: usize = 3;
const MAX_REDIRECTS: usize = 10;

type ClientKey = (Option<String>, bool, bool);

pub struct HttpClientManager {
    clients: RwLock<Arc<Clients>>,
}

struct Clients {
    proxy: Option<Url>,
    default_client: Arc<Client>,
    by_origin: DashMap<ClientKey, Arc<Client>>,
    status_client: OnceLock<Arc<Client>>,
}

impl Clients {
    fn new(proxy: Option<Url>) -> DomainResult<Self> {
        let default_client = HttpClientManager::build_client(false, false, proxy.as_ref())?;
        Ok(Self {
            proxy,
            default_client: Arc::new(default_client),
            by_origin: DashMap::new(),
            status_client: OnceLock::new(),
        })
    }
}

impl HttpClientManager {
    pub fn new() -> DomainResult<Self> {
        Ok(Self {
            clients: RwLock::new(Arc::new(Clients::new(None)?)),
        })
    }

    pub fn set_proxy(&self, proxy_url: &str) -> DomainResult<bool> {
        let proxy = parse_proxy_url(proxy_url)?;
        if self.current().proxy == proxy {
            return Ok(false);
        }

        let clients = Arc::new(Clients::new(proxy)?);
        *self.clients.write().unwrap_or_else(PoisonError::into_inner) = clients;
        Ok(true)
    }

    fn current(&self) -> Arc<Clients> {
        Arc::clone(&self.clients.read().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn client_for(&self, options: &ClientOptions) -> DomainResult<Arc<Client>> {
        let clients = self.current();
        if *options == ClientOptions::default() {
            return Ok(Arc::clone(&clients.default_client));
        }

        let key = (
            options.origin.as_ref().map(normalize_origin),
            options.accept_invalid_certs,
            options.same_host_redirects,
        );
        let entry = clients.by_origin.entry(key).or_try_insert_with(|| {
            Self::build_client(
                options.accept_invalid_certs,
                options.same_host_redirects,
                clients.proxy.as_ref(),
            )
            .map(Arc::new)
        })?;
        Ok(Arc::clone(entry.value()))
    }

    pub fn status_client(&self) -> DomainResult<Arc<Client>> {
        let clients = self.current();
        if let Some(client) = clients.status_client.get() {
            return Ok(Arc::clone(client));
        }
        let client = Arc::new(Self::build_status_client(clients.proxy.as_ref())?);
        Ok(Arc::clone(clients.status_client.get_or_init(|| client)))
    }

    fn base_builder(proxy: Option<&Url>) -> DomainResult<ClientBuilder> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let builder = Client::builder()
            .tls_backend_rustls()
            .https_only(cfg!(target_os = "ios"))
            .user_agent(USER_AGENT);
        let Some(proxy) = proxy else {
            return Ok(builder);
        };

        let proxy = Proxy::all(proxy.clone())
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to use the proxy URL: {e}")))?
            .no_proxy(NoProxy::from_env());
        Ok(builder.proxy(proxy))
    }

    fn build_client(
        accept_invalid_certs: bool, same_host_redirects: bool, proxy: Option<&Url>,
    ) -> DomainResult<Client> {
        let pool_size = std::env::var("PIPEDASH_HTTP_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let mut builder = Self::base_builder(proxy)?;
        if same_host_redirects {
            builder = builder.redirect(redirect::Policy::custom(|attempt| {
                let first = attempt.previous().first();
                if attempt.previous().len() > MAX_REDIRECTS {
                    attempt.error("too many redirects")
                } else if first.is_some_and(|first| stays_on_host(first, attempt.url())) {
                    attempt.follow()
                } else {
                    attempt.error("redirect to another host refused")
                }
            }));
        }
        builder
            .tls_danger_accept_invalid_certs(accept_invalid_certs)
            .pool_max_idle_per_host(pool_size)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| DomainError::InternalError(format!("Failed to create HTTP client: {}", e)))
    }

    fn build_status_client(proxy: Option<&Url>) -> DomainResult<Client> {
        let policy = redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > STATUS_MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if is_safe_redirect_target(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirect target refused")
            }
        });

        Self::base_builder(proxy)?
            .redirect(policy)
            .timeout(STATUS_TIMEOUT)
            .connect_timeout(STATUS_TIMEOUT)
            .build()
            .map_err(|e| {
                DomainError::InternalError(format!("Failed to create status HTTP client: {}", e))
            })
    }
}

pub fn parse_proxy_url(value: &str) -> DomainResult<Option<Url>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    let url = Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .ok_or_else(|| {
            DomainError::InvalidConfig(
                "Proxy URL must look like http://proxy.example.com:3128 or https://...".to_string(),
            )
        })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(DomainError::InvalidConfig(
            "Proxy URL can't contain a username or password. Pipedash supports proxies without \
             authentication only."
                .to_string(),
        ));
    }

    Ok(Some(url))
}

fn normalize_origin(url: &Url) -> String {
    format!(
        "{}://{}:{}",
        url.scheme().to_ascii_lowercase(),
        url.host_str().unwrap_or_default().to_ascii_lowercase(),
        url.port_or_known_default().unwrap_or_default()
    )
}

fn stays_on_host(first: &Url, next: &Url) -> bool {
    first.host_str() == next.host_str() && (first.scheme() != "https" || next.scheme() == "https")
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
mod tests;
