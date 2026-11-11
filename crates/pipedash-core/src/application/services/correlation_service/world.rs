use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
    HashSet,
};
use std::sync::Arc;

use pipedash_plugin_api::PathScope;

use super::names::name_key;
use super::stored::{
    Identity,
    StoredOverride,
};
use super::vocab::target_node_id;
use super::CorrelationService;
use crate::domain::{
    pipeline_id,
    DomainResult,
};
use crate::infrastructure::database::{
    ApplicationMemberRow,
    ApplicationRow,
    ArtifactObservationRow,
    ArtifactRow,
    CorrelationConn,
    SourceObservationRow,
    SubjectIdentityRow,
    TargetObservationRow,
    PIPELINE_LEVEL,
};
use crate::infrastructure::PipedashConfig;

#[derive(Debug, Clone)]
pub(super) struct SourceRef {
    pub repo: String,
    pub scopes: Vec<String>,
    pub commit: Option<String>,
}

impl SourceRef {
    fn roots(&self) -> Vec<&str> {
        if self.scopes.is_empty() {
            vec![""]
        } else {
            self.scopes.iter().map(String::as_str).collect()
        }
    }

    pub(super) fn path(&self) -> &str {
        self.scopes.first().map_or("", String::as_str)
    }

    pub(super) fn overlaps(&self, other: &SourceRef) -> bool {
        self.repo == other.repo
            && self.roots().iter().any(|a| {
                other
                    .roots()
                    .iter()
                    .any(|b| PathScope::new(a).overlaps(&PathScope::new(b)))
            })
    }
}

#[derive(Debug, Clone)]
pub(super) struct TargetRef {
    pub cluster: String,
    pub namespace: Option<String>,
    pub name: String,
    pub short_lived: bool,
}

impl TargetRef {
    pub(super) fn key(&self) -> String {
        target_node_id(&self.cluster, self.namespace.as_deref())
    }
}

#[derive(Debug, Clone)]
pub(super) struct ArtifactRef {
    pub repository: String,
    pub digest: Option<String>,
    pub tag: Option<String>,
    pub target: Option<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct Subject {
    pub id: String,
    pub unit: Option<String>,
    pub parent: Option<String>,
    pub depends_on: Vec<String>,
    pub app_ref: Option<(String, String)>,
    pub label: Option<String>,
    pub sources: Vec<SourceRef>,
    pub targets: Vec<TargetRef>,
    pub consumed: Vec<ArtifactRef>,
    pub produced: Vec<ArtifactRef>,
}

impl Subject {
    pub(super) fn is_build(&self) -> bool {
        self.unit.is_none() && self.targets.is_empty()
    }

    pub(super) fn consumed_digests(&self) -> BTreeSet<&str> {
        self.consumed
            .iter()
            .filter_map(|a| a.digest.as_deref())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct PipelineInfo {
    pub name: String,
    pub provider_id: i64,
    pub provider_type: String,
}

pub(super) struct World {
    pub subjects: BTreeMap<String, Subject>,
    pub units: BTreeMap<String, String>,
    pub pipelines: BTreeMap<String, PipelineInfo>,
    pub artifacts: BTreeMap<String, ArtifactRow>,
    pub providers: BTreeMap<i64, String>,
    pub config: Arc<PipedashConfig>,
    pub overrides: Vec<StoredOverride>,
    pub applications: BTreeMap<String, ApplicationRow>,
    pub members: BTreeMap<i64, Vec<ApplicationMemberRow>>,
}

pub(super) struct Affected {
    pub all: bool,
    pub subjects: BTreeSet<String>,
}

impl Affected {
    pub(super) fn everything() -> Self {
        Self {
            all: true,
            subjects: BTreeSet::new(),
        }
    }

    pub(super) fn has(&self, subject: &str) -> bool {
        self.all || self.subjects.contains(subject)
    }
}

fn group<R>(rows: Vec<R>, subject: impl Fn(&R) -> &str) -> HashMap<String, Vec<R>> {
    let mut grouped: HashMap<String, Vec<R>> = HashMap::new();
    for row in rows {
        grouped
            .entry(subject(&row).to_string())
            .or_default()
            .push(row);
    }
    grouped
}

fn scopes_of(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn subject_of(
    identity: SubjectIdentityRow, sources: Vec<SourceObservationRow>,
    targets: Vec<TargetObservationRow>, artifacts: Vec<ArtifactObservationRow>,
) -> Subject {
    let app_ref = match (
        &identity.app_ref_authority,
        &identity.app_ref_scope,
        &identity.app_ref_id,
    ) {
        (Some(authority), Some(scope), Some(id)) => {
            Some((format!("{authority}|{scope}|{id}"), id.clone()))
        }
        _ => None,
    };
    let artifact = |row: &ArtifactObservationRow| ArtifactRef {
        repository: row.repository.clone(),
        digest: row.digest.clone(),
        tag: row.tag.clone(),
        target: row.target_ordinal.and_then(|o| usize::try_from(o).ok()),
    };
    Subject {
        unit: identity.unit_id,
        parent: identity.unit_parent,
        depends_on: serde_json::from_str(&identity.unit_depends_on_json).unwrap_or_default(),
        app_ref,
        label: identity.app_label,
        sources: sources
            .into_iter()
            .map(|row| SourceRef {
                scopes: scopes_of(&row.path_scopes_json),
                repo: row.repo,
                commit: row.commit_sha,
            })
            .collect(),
        targets: targets
            .into_iter()
            .map(|row| TargetRef {
                short_lived: row.lifetime == "short_lived",
                cluster: row.cluster,
                namespace: row.namespace,
                name: row.name,
            })
            .collect(),
        consumed: artifacts
            .iter()
            .filter(|row| row.relation == "consumed")
            .map(artifact)
            .collect(),
        produced: artifacts
            .iter()
            .filter(|row| row.relation == "produced")
            .map(artifact)
            .collect(),
        id: identity.subject_id,
    }
}

impl CorrelationService {
    pub(super) async fn load_world(&self, conn: &mut CorrelationConn) -> DomainResult<World> {
        let identities = conn.list_subject_identities().await?;
        let mut sources = group(conn.list_pipeline_sources(None).await?, |r| &r.subject_id);
        let mut targets = group(conn.list_pipeline_targets(None).await?, |r| &r.subject_id);
        let mut artifacts = group(conn.list_pipeline_artifacts(None).await?, |r| &r.subject_id);
        let mut subjects = BTreeMap::new();
        let mut units = BTreeMap::new();
        for identity in identities {
            let id = identity.subject_id.clone();
            let subject = subject_of(
                identity,
                sources.remove(&id).unwrap_or_default(),
                targets.remove(&id).unwrap_or_default(),
                artifacts.remove(&id).unwrap_or_default(),
            );
            if let Some(unit) = &subject.unit {
                units.insert(unit.clone(), id.clone());
            }
            subjects.insert(id, subject);
        }
        let pipelines = self
            .repository
            .get_cached_pipelines(None)
            .await?
            .into_iter()
            .map(|p| {
                (
                    p.id,
                    PipelineInfo {
                        name: p.name,
                        provider_id: p.provider_id,
                        provider_type: p.provider_type,
                    },
                )
            })
            .collect();
        let providers = self
            .repository
            .list_providers()
            .await?
            .into_iter()
            .filter_map(|p| p.id.map(|id| (id, p.name)))
            .collect();
        let overrides = conn
            .list_overrides()
            .await?
            .iter()
            .filter_map(StoredOverride::parse)
            .collect();
        let applications = conn
            .list_applications()
            .await?
            .into_iter()
            .filter_map(|row| Identity::of_row(&row).map(|i| (i.encode(), row)))
            .collect();
        let mut members: BTreeMap<i64, Vec<ApplicationMemberRow>> = BTreeMap::new();
        for row in conn.list_application_members().await? {
            members.entry(row.application_id).or_default().push(row);
        }
        Ok(World {
            subjects,
            units,
            pipelines,
            artifacts: conn
                .list_artifacts()
                .await?
                .into_iter()
                .map(|a| (a.digest.clone(), a))
                .collect(),
            providers,
            config: self.config_state.get().await,
            overrides,
            applications,
            members,
        })
    }

    pub(super) async fn affected(
        &self, conn: &mut CorrelationConn, world: &World, changed: &HashSet<String>,
    ) -> DomainResult<Affected> {
        let index = world.index();
        let mut subjects: BTreeSet<String> = changed.iter().cloned().collect();
        for id in changed {
            let mut keys = world
                .subjects
                .get(id)
                .map(|s| world.keys(s))
                .unwrap_or_default();
            for row in conn.list_artifact_observations_by_subject(id).await? {
                if row.run_number != PIPELINE_LEVEL && row.relation == "produced" {
                    keys.extend(row.digest.map(|d| format!("digest:{d}")));
                }
            }
            for key in keys {
                subjects.extend(index.get(&key).into_iter().flatten().cloned());
            }
        }
        Ok(Affected {
            all: false,
            subjects,
        })
    }
}

impl World {
    pub(super) fn name_of(&self, pipeline: &str) -> String {
        self.pipelines
            .get(pipeline)
            .map_or_else(|| pipeline.to_string(), |p| p.name.clone())
    }

    pub(super) fn provider_key(&self, pipeline: &str) -> Option<&str> {
        let info = self.pipelines.get(pipeline)?;
        self.providers.get(&info.provider_id).map(String::as_str)
    }

    pub(super) fn local_id<'a>(&self, pipeline: &'a str) -> Option<&'a str> {
        let info = self.pipelines.get(pipeline)?;
        pipeline_id::strip(&info.provider_type, info.provider_id, pipeline).ok()
    }

    pub(super) fn keys(&self, subject: &Subject) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        keys.extend(subject.sources.iter().map(|s| format!("repo:{}", s.repo)));
        keys.extend(
            subject
                .consumed
                .iter()
                .chain(&subject.produced)
                .filter_map(|a| a.digest.as_ref())
                .map(|d| format!("digest:{d}")),
        );
        if let Some(unit) = &subject.unit {
            keys.insert(format!("unit:{unit}"));
            let name = self.name_of(&subject.id);
            keys.extend(name_key(&name).map(|n| format!("name:{n}")));
        }
        keys.extend(subject.app_ref.iter().map(|(k, _)| format!("app_ref:{k}")));
        keys.extend(subject.label.iter().map(|l| format!("label:{l}")));
        keys
    }

    pub(super) fn index(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for subject in self.subjects.values() {
            for key in self.keys(subject) {
                index.entry(key).or_default().insert(subject.id.clone());
            }
        }
        index
    }

    pub(super) fn unit_subjects(&self) -> impl Iterator<Item = (&String, &Subject)> {
        self.units
            .iter()
            .filter_map(|(key, id)| self.subjects.get(id).map(|s| (key, s)))
    }

    pub(super) fn builders(&self) -> impl Iterator<Item = &Subject> {
        self.subjects.values().filter(|s| s.is_build())
    }
}
