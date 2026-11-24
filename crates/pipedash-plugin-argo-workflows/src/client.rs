use kube::api::ListParams;
use kube::core::Request;
use pipedash_plugin_api::kubernetes;
use pipedash_plugin_api::{
    PluginResult,
    RetryPolicy,
};
use serde::de::DeserializeOwned;

use crate::config::{
    Kind,
    PipelineRef,
};
use crate::types::{
    CronWorkflow,
    List,
    Workflow,
    WorkflowTemplate,
};

const API: &str = "/apis/argoproj.io/v1alpha1";

pub(crate) struct ArgoClient {
    client: kube::Client,
    retry_policy: RetryPolicy,
}

fn scope(namespace: Option<&str>) -> String {
    namespace
        .map(|namespace| format!("{API}/namespaces/{namespace}"))
        .unwrap_or_else(|| API.to_string())
}

impl ArgoClient {
    pub fn new(client: kube::Client) -> Self {
        Self {
            client,
            retry_policy: RetryPolicy::default(),
        }
    }

    async fn list<T: DeserializeOwned>(
        &self, path: String, params: ListParams, context: &str,
    ) -> PluginResult<Vec<T>> {
        self.retry_policy
            .retry(|| async {
                let request = Request::new(&path).list(&params);
                let list: List<T> = kubernetes::exec(&self.client, request, context).await?;
                Ok(list.items)
            })
            .await
    }

    pub async fn list_templates(
        &self, namespace: Option<&str>,
    ) -> PluginResult<Vec<WorkflowTemplate>> {
        let path = format!("{}/{}", scope(namespace), Kind::Template.resource());
        self.list(
            path,
            ListParams::default(),
            "Failed to list workflow templates",
        )
        .await
    }

    pub async fn list_cron_workflows(
        &self, namespace: Option<&str>,
    ) -> PluginResult<Vec<CronWorkflow>> {
        let path = format!("{}/{}", scope(namespace), Kind::Cron.resource());
        self.list(path, ListParams::default(), "Failed to list cron workflows")
            .await
    }

    pub async fn list_workflows(&self, pipeline: &PipelineRef) -> PluginResult<Vec<Workflow>> {
        let path = format!("{}/workflows", scope(Some(&pipeline.namespace)));
        let selector = format!("{}={}", pipeline.kind.run_label(), pipeline.name);
        self.list(
            path,
            ListParams::default().labels(&selector),
            "Failed to list workflows",
        )
        .await
    }
}
