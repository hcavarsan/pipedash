use std::collections::{
    BTreeSet,
    HashMap,
};

use futures::{
    stream,
    StreamExt,
};
use pipedash_plugin_api::{
    ArtifactObservation,
    ClusterId,
    RepoId,
};

use super::{
    FluxcdPlugin,
    CONCURRENCY,
};
use crate::{
    client::FluxClient,
    config::Target,
    mapper::{
        self,
        correlation::Facts,
    },
    pods::{
        self,
        Live,
    },
    types::{
        FluxResource,
        Kind,
    },
};

async fn git_repo(client: &FluxClient, resource: &FluxResource) -> Option<RepoId> {
    let reference = mapper::correlation::git_source(resource)?;

    match client
        .source_url(reference, &resource.metadata.namespace)
        .await
    {
        Ok(url) => url.as_deref().and_then(RepoId::parse),
        Err(error) => {
            tracing::debug!(%error, "Git source unavailable; emitting no source observation");
            None
        }
    }
}

async fn load_live(client: &FluxClient, found: &[FluxResource]) -> HashMap<String, Live> {
    let namespaces: BTreeSet<String> = found
        .iter()
        .filter(|resource| resource.kind == Kind::Kustomization)
        .flat_map(pods::workloads)
        .map(|workload| workload.namespace)
        .collect();

    stream::iter(namespaces)
        .map(|namespace| async move {
            let live = client.live(&namespace).await;
            (namespace, live)
        })
        .buffer_unordered(CONCURRENCY)
        .filter_map(|(namespace, live)| async move {
            match live {
                Ok(live) => Some((namespace, live)),
                Err(error) => {
                    tracing::debug!(%error, %namespace, "Pods unavailable; emitting no consumed images");
                    None
                }
            }
        })
        .collect()
        .await
}

async fn consumed_images(
    client: &FluxClient, resource: &FluxResource, live: &HashMap<String, Live>,
) -> Vec<ArtifactObservation> {
    match resource.kind {
        Kind::Kustomization => {
            let workloads = pods::workloads(resource);
            let namespaces: BTreeSet<&str> = workloads
                .iter()
                .map(|workload| workload.namespace.as_str())
                .collect();
            let loaded: Vec<&Live> = namespaces
                .into_iter()
                .filter_map(|namespace| live.get(namespace))
                .collect();

            pods::images(pods::owned_pods(&workloads, &loaded))
        }
        Kind::HelmRelease => {
            let group = Kind::HelmRelease.group();
            let selector = format!(
                "{group}/name={},{group}/namespace={}",
                resource.metadata.name, resource.metadata.namespace
            );

            match client
                .pods(resource.target_namespace(), Some(&selector))
                .await
            {
                Ok(found) => pods::images(&found),
                Err(error) => {
                    tracing::debug!(%error, "Pods unavailable; emitting no consumed images");
                    Vec::new()
                }
            }
        }
    }
}

async fn pipeline_fact(
    client: &FluxClient, resource: &FluxResource, cluster: Option<&ClusterId>,
    live: &HashMap<String, Live>,
) -> (Target, Facts) {
    let images = if cluster.is_some() {
        consumed_images(client, resource, live).await
    } else {
        Vec::new()
    };
    let facts = Facts {
        repo: git_repo(client, resource).await,
        cluster: cluster.cloned(),
        images,
    };

    (mapper::target_of(resource), facts)
}

impl FluxcdPlugin {
    pub(super) async fn run_facts(&self, resource: &FluxResource) -> Facts {
        let Ok(client) = self.client().await else {
            return Facts::default();
        };

        Facts {
            repo: git_repo(client, resource).await,
            ..Facts::default()
        }
    }

    pub(super) async fn pipeline_facts(&self, found: &[FluxResource]) -> HashMap<Target, Facts> {
        let Ok(client) = self.client().await else {
            return HashMap::new();
        };
        let cluster = client.cluster_id().await.unwrap_or_else(|error| {
            tracing::debug!(%error, "Cluster identity unavailable; emitting no deploy target");
            None
        });
        let live = if cluster.is_some() {
            load_live(client, found).await
        } else {
            HashMap::new()
        };
        let cluster = cluster.as_ref();
        let pending: Vec<_> = found
            .iter()
            .map(|resource| pipeline_fact(client, resource, cluster, &live))
            .collect();

        stream::iter(pending)
            .buffer_unordered(CONCURRENCY)
            .collect()
            .await
    }
}
