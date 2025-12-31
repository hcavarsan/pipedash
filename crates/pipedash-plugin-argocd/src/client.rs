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

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ArgocdClient {
    http_client: std::sync::Arc<reqwest::Client>,
    api_url: String,
    auth_header: String,
    retry_policy: RetryPolicy,
}

impl ArgocdClient {
    pub fn new(
        http_client: Option<std::sync::Arc<reqwest::Client>>, server_url: String, token: String,
        insecure: bool,
    ) -> PluginResult<Self> {
        let api_url = Self::build_api_url(&server_url);
        let auth_header = format!("Bearer {}", token);

        let client = http_client.unwrap_or_else(|| {
            std::sync::Arc::new(
                reqwest::Client::builder()
                    .use_rustls_tls()
                    .danger_accept_invalid_certs(insecure)
                    .pool_max_idle_per_host(10)
                    .timeout(DEFAULT_REQUEST_TIMEOUT)
                    .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
                    .tcp_keepalive(Duration::from_secs(60))
                    .build()
                    .expect("Failed to build HTTP client"),
            )
        });

        Ok(Self {
            http_client: client,
            api_url,
            auth_header,
            retry_policy: RetryPolicy::default(),
        })
    }

    fn build_api_url(server_url: &str) -> String {
        format!("{}/api/v1", server_url.trim_end_matches('/'))
    }

    pub async fn list_applications(
        &self, projects_filter: Option<&Vec<String>>,
    ) -> PluginResult<Vec<Application>> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/applications", self.api_url);
                let response = self
                    .http_client
                    .get(&url)
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::NetworkError(format!("Failed to list applications: {}", e))
                    })?;

                let app_list: ApplicationList = self.handle_response(response).await?;

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

    pub async fn get_application(&self, app_name: &str) -> PluginResult<Application> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/applications/{}", self.api_url, app_name);
                let response = self
                    .http_client
                    .get(&url)
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::NetworkError(format!(
                            "Failed to get application '{}': {}",
                            app_name, e
                        ))
                    })?;

                self.handle_response(response).await
            })
            .await
    }

    pub async fn sync_application(
        &self, app_name: &str, revision: Option<String>, prune: bool, dry_run: bool, force: bool,
        apply_only: bool,
    ) -> PluginResult<()> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/applications/{}/sync", self.api_url, app_name);

                let strategy = if force || apply_only {
                    Some(crate::types::SyncStrategy {
                        apply: if force {
                            Some(crate::types::SyncStrategyApply { force: Some(true) })
                        } else {
                            None
                        },
                        hook: if apply_only {
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
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
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
                let response = self
                    .http_client
                    .delete(&url)
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::NetworkError(format!(
                            "Failed to terminate operation for application '{}': {}",
                            app_name, e
                        ))
                    })?;

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
