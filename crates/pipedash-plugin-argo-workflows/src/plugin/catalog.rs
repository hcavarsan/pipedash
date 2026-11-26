use std::collections::HashSet;

use pipedash_plugin_api::support::{
    try_buffered,
    CONCURRENCY,
};
use pipedash_plugin_api::{
    Pipeline,
    PluginResult,
};

use super::ArgoWorkflowsPlugin;
use crate::client::ArgoClient;
use crate::config::{
    self,
    Kind,
    PipelineRef,
};
use crate::mapper;
use crate::types::CronSpec;

pub(super) struct Entry {
    pub pipeline: PipelineRef,
    pub cron: Option<CronSpec>,
}

async fn entries_in(client: &ArgoClient, namespace: Option<&str>) -> PluginResult<Vec<Entry>> {
    let (templates, crons) = tokio::try_join!(
        client.list_templates(namespace),
        client.list_cron_workflows(namespace)
    )?;
    let templates = templates.into_iter().map(|template| Entry {
        pipeline: PipelineRef::new(
            &template.metadata.namespace,
            Kind::Template,
            &template.metadata.name,
        ),
        cron: None,
    });
    let crons = crons.into_iter().map(|cron| Entry {
        pipeline: PipelineRef::new(&cron.metadata.namespace, Kind::Cron, &cron.metadata.name),
        cron: Some(cron.spec),
    });
    Ok(templates.chain(crons).collect())
}

impl ArgoWorkflowsPlugin {
    pub(super) async fn entries(&self, namespaces: Vec<String>) -> PluginResult<Vec<Entry>> {
        let client = self.client().await?;
        let scopes: Vec<Option<String>> = if namespaces.is_empty() {
            vec![None]
        } else {
            namespaces.into_iter().map(Some).collect()
        };
        let requests = scopes
            .iter()
            .map(|namespace| entries_in(client, namespace.as_deref()))
            .collect();
        Ok(try_buffered(requests, CONCURRENCY)
            .await?
            .into_iter()
            .flatten()
            .collect())
    }

    pub(super) async fn configured_entries(&self) -> PluginResult<Vec<Entry>> {
        self.entries(config::namespaces(&self.config)).await
    }

    pub(super) async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        let selected: HashSet<PipelineRef> = config::selected(&self.config);
        if selected.is_empty() {
            return Ok(Vec::new());
        }
        let namespaces: HashSet<String> = selected
            .iter()
            .map(|pipeline| pipeline.namespace.clone())
            .collect();
        let client = self.client().await?;
        let requests = self
            .entries(namespaces.into_iter().collect())
            .await?
            .into_iter()
            .filter(|entry| selected.contains(&entry.pipeline))
            .map(|entry| async move {
                let mut workflows = client.list_workflows(&entry.pipeline).await?;
                mapper::newest_first(&mut workflows);
                Ok(mapper::map_pipeline(
                    &entry.pipeline,
                    entry.cron.as_ref(),
                    workflows.first(),
                ))
            })
            .collect();
        try_buffered(requests, CONCURRENCY).await
    }
}
