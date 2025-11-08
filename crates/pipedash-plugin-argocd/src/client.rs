use std::time::Duration;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::StatusCode;
use tracing::debug;

use crate::types::{
    Application,
    ApplicationList,
    SyncRequest,
};

/// Default timeout for HTTP requests (30 seconds)
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Default timeout for establishing connections (10 seconds)
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ArgocdClient {
    http_client: reqwest::Client,
    api_url: String,
    retry_policy: RetryPolicy,
}

impl ArgocdClient {
    pub fn new(server_url: String, token: String, insecure: bool) -> PluginResult<Self> {
        let api_url = Self::build_api_url(&server_url);
        let headers = Self::build_auth_headers(&token)?;
        let http_client = Self::build_http_client(headers, insecure)?;

        Ok(Self {
            http_client,
            api_url,
            retry_policy: RetryPolicy::default(),
        })
    }

    /// Build the API URL from the server URL
    fn build_api_url(server_url: &str) -> String {
        format!("{}/api/v1", server_url.trim_end_matches('/'))
    }

    /// Build authentication headers with Bearer token
    fn build_auth_headers(token: &str) -> PluginResult<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        let auth_value = format!("Bearer {}", token);
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&auth_value)
                .map_err(|e| PluginError::InvalidConfig(format!("Invalid token format: {}", e)))?,
        );
        Ok(headers)
    }

    /// Build the HTTP client with configured timeout and TLS settings
    fn build_http_client(
        headers: reqwest::header::HeaderMap, insecure: bool,
    ) -> PluginResult<reqwest::Client> {
        reqwest::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(insecure)
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .build()
            .map_err(|e| PluginError::Internal(format!("Failed to create HTTP client: {}", e)))
    }

    /// List all applications, optionally filtered by projects
    pub async fn list_applications(
        &self, projects_filter: Option<&Vec<String>>,
    ) -> PluginResult<Vec<Application>> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/applications", self.api_url);
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list applications: {}", e))
                })?;

                let app_list: ApplicationList = self.handle_response(response).await?;

                // Filter by projects if specified
                let filtered_apps = if let Some(projects) = projects_filter {
                    app_list
                        .items
                        .into_iter()
                        .filter(|app| projects.contains(&app.spec.project))
                        .collect()
                } else {
                    app_list.items
                };

                Ok(filtered_apps)
            })
            .await
    }

    /// Get a specific application by name
    pub async fn get_application(&self, app_name: &str) -> PluginResult<Application> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/applications/{}", self.api_url, app_name);
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!(
                        "Failed to get application '{}': {}",
                        app_name, e
                    ))
                })?;

                self.handle_response(response).await
            })
            .await
    }

    /// Trigger a sync operation for an application with full ArgoCD options
    pub async fn sync_application(
        &self, app_name: &str, revision: Option<String>, prune: bool, dry_run: bool, force: bool,
        apply_only: bool,
    ) -> PluginResult<()> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/applications/{}/sync", self.api_url, app_name);

                // Build strategy if force or apply_only is set
                let strategy = if force || apply_only {
                    Some(crate::types::SyncStrategy {
                        apply: if force {
                            Some(crate::types::SyncStrategyApply { force: Some(true) })
                        } else {
                            None
                        },
                        hook: if apply_only {
                            // apply_only means skip hooks, so force = false
                            Some(crate::types::SyncStrategyHook { force: Some(false) })
                        } else {
                            None
                        },
                    })
                } else {
                    None
                };

                let sync_request = SyncRequest {
                    revision: revision.clone(),
                    prune: Some(prune),
                    dry_run: Some(dry_run),
                    strategy,
                };

                debug!(?sync_request, "Sending sync request to ArgoCD API");

                let response = self
                    .http_client
                    .post(&url)
                    .json(&sync_request)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::NetworkError(format!(
                            "Failed to sync application '{}': {}",
                            app_name, e
                        ))
                    })?;

                self.handle_response::<serde_json::Value>(response).await?;
                Ok(())
            })
            .await
    }

    pub async fn terminate_operation(&self, app_name: &str) -> PluginResult<()> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/applications/{}/operation", self.api_url, app_name);
                let response = self.http_client.delete(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!(
                        "Failed to terminate operation for application '{}': {}",
                        app_name, e
                    ))
                })?;

                // Check status code
                let status = response.status();
                if !status.is_success() {
                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(PluginError::ApiError(format!(
                        "Failed to terminate operation for application '{}' (status {}): {}",
                        app_name, status, error_text
                    )));
                }

                Ok(())
            })
            .await
    }

    /// Handle HTTP response and deserialize to type T
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self, response: reqwest::Response,
    ) -> PluginResult<T> {
        let status = response.status();
        let url = response.url().clone();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::AuthenticationFailed(format!(
                "Authentication failed for {}: {}",
                url, error_text
            )));
        }

        if status == StatusCode::NOT_FOUND {
            return Err(PluginError::PipelineNotFound(format!(
                "Resource not found: {}",
                url
            )));
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::ApiError(format!(
                "ArgoCD API error ({}) for {}: {}",
                status, url, error_text
            )));
        }

        response.json::<T>().await.map_err(|e| {
            PluginError::ApiError(format!(
                "Failed to parse ArgoCD API response from {}: {}",
                url, e
            ))
        })
    }
}
