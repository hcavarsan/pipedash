use std::sync::Arc;

use pipedash_plugin_api::{
    check_response,
    map_send_error,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde_json::{
    json,
    Value,
};

use crate::types::{
    ListUpdatesResponse,
    StackListItem,
    Update,
    UpdateListItem,
    VersionResponse,
};

pub(crate) struct KomodoClient {
    http_client: Arc<reqwest::Client>,
    server_url: String,
    api_key: String,
    api_secret: String,
    retry_policy: RetryPolicy,
}

impl KomodoClient {
    pub fn new(
        http_client: Arc<reqwest::Client>, server_url: String, api_key: String, api_secret: String,
    ) -> Self {
        Self {
            http_client,
            server_url,
            api_key,
            api_secret,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub async fn version(&self) -> PluginResult<String> {
        let response: VersionResponse = self.read("GetVersion", json!({})).await?;
        Ok(response.version)
    }

    pub async fn list_stacks(&self) -> PluginResult<Vec<StackListItem>> {
        let stacks: Vec<StackListItem> = self
            .read("ListStacks", json!({ "query": {}, "limit": 0 }))
            .await?;
        Ok(stacks.into_iter().filter(|stack| !stack.template).collect())
    }

    pub async fn stack_updates(
        &self, stack_id: &str, limit: usize,
    ) -> PluginResult<Vec<UpdateListItem>> {
        let mut updates = Vec::new();
        let mut page = 0;
        while updates.len() < limit {
            let response = self.list_updates(stack_target(stack_id), page).await?;
            updates.extend(response.updates);
            match response.next_page {
                Some(next) if next > page => page = next,
                _ => break,
            }
        }
        updates.truncate(limit);
        Ok(updates)
    }

    pub async fn updates_started_at(
        &self, stack_id: &str, start_ts: i64,
    ) -> PluginResult<Vec<UpdateListItem>> {
        let mut query = stack_target(stack_id);
        query["start_ts"] = json!(start_ts);
        Ok(self.list_updates(query, 0).await?.updates)
    }

    pub async fn update(&self, update_id: &str) -> PluginResult<Update> {
        self.read("GetUpdate", json!({ "id": update_id })).await
    }

    pub async fn deploy_stack(&self, stack_id: &str) -> PluginResult<Update> {
        let url = self.url("execute")?;
        let params = json!({ "stack": stack_id });
        let response = self
            .retry_policy
            .retry_unsent(&url, || self.request(&url, "DeployStack", &params).send())
            .await?;
        parse_json(check_response(response).await?, "DeployStack").await
    }

    async fn list_updates(&self, query: Value, page: u32) -> PluginResult<ListUpdatesResponse> {
        self.read("ListUpdates", json!({ "query": query, "page": page }))
            .await
    }

    async fn read<T: DeserializeOwned>(
        &self, request_type: &str, params: Value,
    ) -> PluginResult<T> {
        let url = self.url("read")?;
        self.retry_policy
            .retry(|| async {
                let response = self
                    .request(&url, request_type, &params)
                    .send()
                    .await
                    .map_err(|e| map_send_error(e, &url))?;
                parse_json(check_response(response).await?, request_type).await
            })
            .await
    }

    fn url(&self, scope: &str) -> PluginResult<Url> {
        Url::parse(&format!("{}/{scope}", self.server_url))
            .map_err(|e| PluginError::InvalidConfig(format!("Invalid Komodo URL: {e}")))
    }

    fn request(&self, url: &Url, request_type: &str, params: &Value) -> reqwest::RequestBuilder {
        self.http_client
            .post(url.clone())
            .header("X-Api-Key", &self.api_key)
            .header("X-Api-Secret", &self.api_secret)
            .json(&json!({ "type": request_type, "params": params }))
    }
}

fn stack_target(stack_id: &str) -> Value {
    json!({ "target.type": "Stack", "target.id": stack_id })
}

async fn parse_json<T: DeserializeOwned>(
    response: reqwest::Response, request_type: &str,
) -> PluginResult<T> {
    response.json().await.map_err(|e| {
        PluginError::ApiError(format!(
            "Failed to parse Komodo {request_type} response: {}",
            e.without_url()
        ))
    })
}
