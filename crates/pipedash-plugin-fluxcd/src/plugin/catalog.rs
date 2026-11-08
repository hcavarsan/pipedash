use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};

use chrono::Utc;
use futures::{
    stream,
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineStatus,
    PluginError,
    PluginResult,
    ResourceState,
};
use serde_json::json;

use super::{
    FluxcdPlugin,
    CONCURRENCY,
};
use crate::{
    config::Target,
    mapper,
    types::{
        FluxResource,
        Kind,
    },
    workloads::{
        Lookup,
        WorkloadKind,
    },
};

pub(super) fn flux_not_installed() -> PluginError {
    PluginError::InvalidConfig(
        "The Flux Kustomization (kustomize.toolkit.fluxcd.io/v1) and HelmRelease (helm.toolkit.fluxcd.io/v2) APIs are not served by this cluster. Install Flux (https://fluxcd.io/flux/installation/) or select the kubeconfig context where Flux runs.".to_string(),
    )
}

pub(super) fn missing_pipeline(target: &Target) -> Pipeline {
    let message = format!(
        "{target} was not found in this cluster. Update your resource selection or recreate it in Flux."
    );

    Pipeline {
        id: target.selection_id(),
        name: format!("{}/{}", target.kind, target.name),
        status: PipelineStatus::Pending,
        last_run: None,
        last_updated: Utc::now(),
        repository: format!("{}/(missing)", target.namespace),
        branch: None,
        workflow_file: None,
        resource_state: Some(ResourceState::new("Not found", Some(&message))),
        metadata: HashMap::from([
            ("kind".to_string(), json!(target.kind.as_str())),
            ("namespace".to_string(), json!(target.namespace)),
            ("name".to_string(), json!(target.name)),
            ("missing".to_string(), json!(true)),
            ("reason".to_string(), json!("ResourceNotFound")),
            ("history_scope".to_string(), json!("bounded_reconciliation")),
        ]),
    }
}

impl FluxcdPlugin {
    async fn list_scopes(
        &self, scopes: Vec<(Kind, Option<String>)>,
    ) -> PluginResult<Vec<FluxResource>> {
        let client = self.client().await?;
        let lists: Vec<Option<Vec<FluxResource>>> = stream::iter(scopes)
            .map(|(kind, namespace)| async move { client.list(kind, namespace.as_deref()).await })
            .buffer_unordered(CONCURRENCY)
            .try_collect()
            .await?;
        if lists.iter().all(Option::is_none) {
            return Err(flux_not_installed());
        }

        let mut resources: Vec<FluxResource> = lists.into_iter().flatten().flatten().collect();
        resources.sort_by(|a, b| {
            (&a.metadata.namespace, &a.metadata.name, a.kind).cmp(&(
                &b.metadata.namespace,
                &b.metadata.name,
                b.kind,
            ))
        });

        Ok(resources)
    }

    pub(super) async fn discover(&self, namespaces: &[String]) -> PluginResult<Vec<FluxResource>> {
        let scopes: Vec<(Kind, Option<String>)> = if namespaces.is_empty() {
            Kind::ALL.iter().map(|kind| (*kind, None)).collect()
        } else {
            namespaces
                .iter()
                .flat_map(|namespace| {
                    Kind::ALL
                        .iter()
                        .map(move |kind| (*kind, Some(namespace.clone())))
                })
                .collect()
        };

        self.list_scopes(scopes).await
    }

    async fn selected(
        &self, selection: &[Target],
    ) -> PluginResult<(Vec<FluxResource>, Vec<Target>)> {
        let scopes: BTreeSet<(Kind, &str)> = selection
            .iter()
            .map(|target| (target.kind, target.namespace.as_str()))
            .collect();
        let wanted: HashSet<&Target> = selection.iter().collect();
        let found: Vec<FluxResource> = self
            .list_scopes(
                scopes
                    .into_iter()
                    .map(|(kind, namespace)| (kind, Some(namespace.to_string())))
                    .collect(),
            )
            .await?
            .into_iter()
            .filter(|resource| wanted.contains(&mapper::target_of(resource)))
            .collect();

        let present: HashSet<Target> = found.iter().map(mapper::target_of).collect();
        let missing = selection
            .iter()
            .filter(|target| !present.contains(*target))
            .cloned()
            .collect();

        Ok((found, missing))
    }

    pub(super) async fn monitored(&self) -> PluginResult<(Vec<FluxResource>, Vec<Target>)> {
        let settings = self.settings()?;
        if settings.selection.is_empty() {
            Ok((self.discover(&settings.namespaces).await?, Vec::new()))
        } else {
            self.selected(&settings.selection).await
        }
    }

    pub(super) async fn available(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let resources = self.discover(&self.settings()?.namespaces).await?;

        Ok(resources
            .iter()
            .map(mapper::map_available_pipeline)
            .collect())
    }

    pub(super) async fn workload_issues(
        &self, found: &[FluxResource],
    ) -> HashMap<Target, Vec<String>> {
        let Ok(client) = self.client().await else {
            return HashMap::new();
        };
        let owners: BTreeSet<Kind> = found
            .iter()
            .filter(|resource| mapper::state::current_status(resource) == PipelineStatus::Success)
            .map(|resource| resource.kind)
            .collect();
        let lookups: Vec<Lookup> = owners
            .into_iter()
            .flat_map(|owner| WorkloadKind::ALL.map(|kind| (owner, kind)))
            .collect();

        let results =
            stream::iter(lookups)
                .map(|lookup| async move {
                    (lookup, client.unhealthy_workloads(lookup.0, lookup.1).await)
                })
                .buffer_unordered(CONCURRENCY)
                .collect::<Vec<_>>()
                .await;
        let mut findings = Vec::new();
        let mut failed = Vec::new();
        for (lookup, result) in results {
            match result {
                Ok(found) => findings.extend(found),
                Err(error) => {
                    tracing::debug!(%error, "Workload health unavailable; keeping the Flux-reported state");
                    failed.push(lookup);
                }
            }
        }
        let lasting = self
            .unhealthy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .lasting(findings, &failed, std::time::Instant::now());
        let mut issues: HashMap<Target, Vec<String>> = HashMap::new();
        for finding in lasting {
            issues
                .entry(finding.owner)
                .or_default()
                .push(finding.message);
        }
        issues.values_mut().for_each(|found| found.sort());

        issues
    }
}
