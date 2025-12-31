use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use reqwest::Client;

use crate::domain::{
    DomainError,
    DomainResult,
};

pub struct HttpClientManager {
    default_client: Arc<Client>,
    custom_clients: DashMap<String, Arc<Client>>,
}

impl HttpClientManager {
    pub fn new() -> DomainResult<Self> {
        let default_client = Self::create_optimized_client()?;
        Ok(Self {
            default_client: Arc::new(default_client),
            custom_clients: DashMap::new(),
        })
    }

    pub fn default_client(&self) -> Arc<Client> {
        Arc::clone(&self.default_client)
    }

    pub fn client_for_url(&self, base_url: &str) -> DomainResult<Arc<Client>> {
        if let Some(client) = self.custom_clients.get(base_url) {
            return Ok(Arc::clone(client.value()));
        }

        let client = Self::create_optimized_client()?;
        let client = Arc::new(client);
        self.custom_clients
            .insert(base_url.to_string(), Arc::clone(&client));
        Ok(client)
    }

    fn create_optimized_client() -> DomainResult<Client> {
        let pool_size = std::env::var("PIPEDASH_HTTP_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        Client::builder()
            .use_rustls_tls()
            .pool_max_idle_per_host(pool_size)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| DomainError::InternalError(format!("Failed to create HTTP client: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_manager_creation() {
        let manager = HttpClientManager::new().unwrap();
        let client = manager.default_client();
        assert!(Arc::strong_count(&client) >= 1);
    }

    #[test]
    fn test_client_caching() {
        let manager = HttpClientManager::new().unwrap();

        let client1 = manager.client_for_url("https://api.github.com").unwrap();
        let client2 = manager.client_for_url("https://api.github.com").unwrap();

        assert!(Arc::ptr_eq(&client1, &client2));
    }

    #[test]
    fn test_different_urls_different_clients() {
        let manager = HttpClientManager::new().unwrap();

        let client1 = manager.client_for_url("https://api.github.com").unwrap();
        let client2 = manager.client_for_url("https://gitlab.com").unwrap();

        assert!(!Arc::ptr_eq(&client1, &client2));
    }

    #[test]
    fn test_default_client_separate_from_custom() {
        let manager = HttpClientManager::new().unwrap();

        let default_client = manager.default_client();
        let custom_client = manager.client_for_url("https://api.github.com").unwrap();

        assert!(!Arc::ptr_eq(&default_client, &custom_client));
    }
}
