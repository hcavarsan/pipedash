mod extras;

use std::collections::{
    BTreeMap,
    BTreeSet,
};

use super::layout::{
    arrange,
    split_artifacts,
    target_columns,
    ARTIFACT_COLUMN,
    BUILD_COLUMN,
    FIRST_TARGET_COLUMN,
    SOURCE_COLUMN,
};
use super::{
    unit_groups,
    worst,
    View,
};
use crate::application::services::correlation_service::changes::ChangeData;
use crate::application::services::correlation_service::copy::{
    self,
    views,
};
use crate::application::services::correlation_service::targets::SHORT_LIVED_GROUP;
use crate::application::services::correlation_service::vocab::{
    commit_node_id,
    ARTIFACT,
    COMMIT,
    RUNS,
    SYNCED,
    UNIT,
};
use crate::domain::application::dto::{
    Flow,
    FlowLink,
    FlowNode,
    FlowNodeKind,
};
use crate::domain::application::{
    Confidence,
    Stage,
};
use crate::domain::pipeline::Pipeline;
use crate::infrastructure::database::CorrelationEdgeRow;

fn target_id(group: &str) -> String {
    format!("target:{group}")
}

fn build_id(pipeline: &str) -> String {
    format!("build:{pipeline}")
}

fn source_id(repo: &str) -> String {
    format!("source:{repo}")
}

fn artifact_id(repository: &str) -> String {
    format!("artifact:{repository}")
}

fn find_edge(
    edges: &[CorrelationEdgeRow], from: (&str, &str), to: (&str, &str), relation: &str,
) -> Option<i64> {
    edges
        .iter()
        .find(|e| {
            e.relation == relation
                && (e.from_kind.as_str(), e.from_id.as_str()) == from
                && (e.to_kind.as_str(), e.to_id.as_str()) == to
        })
        .map(|e| e.id)
}

struct Builder<'a, 'v> {
    view: &'v View<'a>,
    nodes: BTreeMap<String, FlowNode>,
    links: BTreeMap<String, FlowLink>,
    short_keys: BTreeSet<&'a str>,
    labels: BTreeMap<&'a str, &'a str>,
}

impl<'a, 'v> Builder<'a, 'v> {
    fn new(view: &'v View<'a>) -> Self {
        let groups = &view.state.groups;
        Self {
            view,
            nodes: BTreeMap::new(),
            links: BTreeMap::new(),
            short_keys: groups
                .iter()
                .filter(|g| g.short_lived)
                .map(|g| g.key.as_str())
                .collect(),
            labels: groups
                .iter()
                .map(|g| (g.key.as_str(), g.label.as_str()))
                .collect(),
        }
    }

    fn put(&mut self, node: FlowNode) {
        self.nodes.entry(node.id.clone()).or_insert(node);
    }

    fn link(&mut self, link: FlowLink) {
        self.links.entry(link.id.clone()).or_insert(link);
    }

    fn exact(from: &str, to: &str, why: String) -> FlowLink {
        FlowLink {
            id: format!("{from}->{to}"),
            from: from.to_string(),
            to: to.to_string(),
            confidence: Confidence::Exact,
            why,
            hint: None,
            edge_id: None,
            suggestion_id: None,
            unlink_pipeline_id: None,
        }
    }

    fn node_of(&self, group: &str) -> String {
        if self.short_keys.contains(group) {
            target_id(SHORT_LIVED_GROUP)
        } else {
            target_id(group)
        }
    }

    fn label_of(&self, group: &str) -> String {
        self.labels
            .get(group)
            .map_or_else(|| group.to_string(), |l| (*l).to_string())
    }

    fn source(&mut self, repo: &str) -> String {
        let id = source_id(repo);
        let (title, subtitle) = views::host_and_path(repo);
        self.put(FlowNode {
            id: id.clone(),
            kind: FlowNodeKind::Source,
            column: SOURCE_COLUMN,
            row: 0,
            title,
            subtitle,
            status: None,
            pipeline_id: None,
            count: 1,
        });
        id
    }

    fn build_node(&mut self, pipeline: &Pipeline) -> String {
        let id = build_id(&pipeline.id);
        self.put(FlowNode {
            id: id.clone(),
            kind: FlowNodeKind::Build,
            column: BUILD_COLUMN,
            row: 0,
            title: pipeline.name.clone(),
            subtitle: (!pipeline.repository.is_empty()).then(|| pipeline.repository.clone()),
            status: Some(pipeline.status.clone()),
            pipeline_id: Some(pipeline.id.clone()),
            count: 1,
        });
        id
    }

    fn targets(&mut self) {
        let state = self.view.state;
        let pipelines = self.view.group_pipelines();
        let mut keys: Vec<String> = state
            .groups
            .iter()
            .filter(|g| !g.short_lived)
            .map(|g| g.key.clone())
            .collect();
        let short_targets: usize = state
            .groups
            .iter()
            .filter(|g| g.short_lived)
            .map(|g| g.targets.len())
            .sum();
        if short_targets > 0 {
            keys.push(SHORT_LIVED_GROUP.to_string());
        }
        let pairs: Vec<(String, String)> = state
            .directions
            .iter()
            .map(|d| (d.from_group.clone(), d.to_group.clone()))
            .collect();
        let columns = target_columns(&keys, &pairs);
        for group in state.groups.iter().filter(|g| !g.short_lived) {
            let found = pipelines
                .get(group.key.as_str())
                .cloned()
                .unwrap_or_default();
            let count = group.targets.len();
            self.put(FlowNode {
                id: target_id(&group.key),
                kind: if count == 1 {
                    FlowNodeKind::Target
                } else {
                    FlowNodeKind::Group
                },
                column: columns
                    .get(&group.key)
                    .copied()
                    .unwrap_or(FIRST_TARGET_COLUMN),
                row: 0,
                title: group.label.clone(),
                subtitle: None,
                status: worst(&found).map(|p| p.status.clone()),
                pipeline_id: match found.as_slice() {
                    [only] => Some(only.id.clone()),
                    _ => None,
                },
                count,
            });
        }
        if short_targets > 0 {
            let found: Vec<_> = state
                .groups
                .iter()
                .filter(|g| g.short_lived)
                .flat_map(|g| pipelines.get(g.key.as_str()).cloned().unwrap_or_default())
                .collect();
            self.put(FlowNode {
                id: target_id(SHORT_LIVED_GROUP),
                kind: FlowNodeKind::ShortLived,
                column: columns
                    .get(SHORT_LIVED_GROUP)
                    .copied()
                    .unwrap_or(FIRST_TARGET_COLUMN),
                row: 0,
                title: copy::short_lived(short_targets),
                subtitle: None,
                status: worst(&found).map(|p| p.status.clone()),
                pipeline_id: None,
                count: short_targets,
            });
        }
        for direction in &state.directions {
            let (from, to) = (
                self.node_of(&direction.from_group),
                self.node_of(&direction.to_group),
            );
            if from == to || !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
                continue;
            }
            let why = views::why_direction(
                &direction.source,
                &self.label_of(&direction.from_group),
                &self.label_of(&direction.to_group),
            );
            self.link(Self::exact(&from, &to, why));
        }
    }

    fn builds(&mut self) {
        let view = self.view;
        for member in view.members(Stage::Build) {
            let Some(pipeline) = view.pipeline(&member.row.pipeline_id) else {
                continue;
            };
            let build = self.build_node(pipeline);
            let Some(subject) = view.facts.world.subjects.get(&pipeline.id) else {
                continue;
            };
            for source in &subject.sources {
                let from = self.source(&source.repo);
                let mut link = Self::exact(&from, &build, views::why_builds_from(&source.repo));
                link.unlink_pipeline_id = Some(pipeline.id.clone());
                self.link(link);
            }
        }
    }

    fn units(&mut self) -> BTreeMap<String, (String, Vec<String>)> {
        let view = self.view;
        let group_of = view.group_of();
        let mut repositories = BTreeSet::new();
        for member in view.members(Stage::Deploy) {
            if let Some(subject) = view.facts.world.subjects.get(&member.row.pipeline_id) {
                repositories.extend(subject.consumed.iter().map(|a| a.repository.clone()));
            }
        }
        let (shown, hidden) = split_artifacts(repositories.into_iter().collect());
        for repository in &shown {
            let (title, subtitle) = views::host_and_path(repository);
            self.put(FlowNode {
                id: artifact_id(repository),
                kind: FlowNodeKind::Artifact,
                column: ARTIFACT_COLUMN,
                row: 0,
                title,
                subtitle,
                status: None,
                pipeline_id: None,
                count: 1,
            });
        }
        if hidden > 0 {
            self.put(FlowNode {
                id: artifact_id("more"),
                kind: FlowNodeKind::Artifact,
                column: ARTIFACT_COLUMN,
                row: 0,
                title: copy::more_artifacts(hidden),
                subtitle: None,
                status: None,
                pipeline_id: None,
                count: hidden,
            });
        }
        let edges = &view.state.edges;
        let mut digests: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
        for member in view.members(Stage::Deploy) {
            let Some(subject) = view.facts.world.subjects.get(&member.row.pipeline_id) else {
                continue;
            };
            let unit = subject.unit.as_deref().unwrap_or_default();
            for source in &subject.sources {
                let from = self.source(&source.repo);
                let edge = source.commit.as_ref().and_then(|sha| {
                    let node = commit_node_id(&source.repo, sha);
                    find_edge(edges, (UNIT, unit), (COMMIT, &node), SYNCED)
                });
                for group in unit_groups(&group_of, subject, None) {
                    let to = self.node_of(group);
                    let mut link = Self::exact(
                        &from,
                        &to,
                        views::why_syncs_from(&source.repo, source.path()),
                    );
                    link.edge_id = edge;
                    self.link(link);
                }
            }
            for artifact in &subject.consumed {
                let Some(digest) = &artifact.digest else {
                    continue;
                };
                let groups = unit_groups(&group_of, subject, artifact.target);
                let entry = digests
                    .entry(digest.clone())
                    .or_insert_with(|| (artifact.repository.clone(), Vec::new()));
                entry.1.extend(groups.iter().map(|g| (*g).to_string()));
                if !shown.contains(&artifact.repository) {
                    continue;
                }
                let from = artifact_id(&artifact.repository);
                let image = views::image_ref(&artifact.repository, digest);
                for group in groups {
                    let to = self.node_of(group);
                    let mut link = Self::exact(&from, &to, copy::why_unit_runs(&image));
                    link.edge_id = find_edge(edges, (UNIT, unit), (ARTIFACT, digest), RUNS);
                    self.link(link);
                }
            }
        }
        digests
    }
}

pub(super) fn build_flow(view: &View<'_>, changes: &[ChangeData]) -> Flow {
    let mut builder = Builder::new(view);
    builder.targets();
    builder.builds();
    let digests = builder.units();
    builder.produced(&digests);
    builder.suggested();
    builder.hints(changes);
    let mut nodes: Vec<FlowNode> = builder.nodes.into_values().collect();
    let (columns, rows) = arrange(&mut nodes);
    Flow {
        columns,
        rows,
        nodes,
        links: builder.links.into_values().collect(),
    }
}
