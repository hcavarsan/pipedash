use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
};

use chrono::{
    DateTime,
    Utc,
};

use super::{
    artifact_id,
    build_id,
    target_id,
    Builder,
};
use crate::application::services::correlation_service::changes::ChangeData;
use crate::application::services::correlation_service::copy;
use crate::application::services::correlation_service::page::layout::FIRST_TARGET_COLUMN;
use crate::application::services::correlation_service::state::OpenSuggestion;
use crate::application::services::correlation_service::vocab::{
    parse_run_node,
    ARTIFACT,
    PRODUCED,
    RUN,
};
use crate::domain::application::dto::{
    FlowLink,
    FlowNode,
    FlowNodeKind,
    SuggestionKind,
};
use crate::domain::application::{
    Confidence,
    Stage,
};

type Digests = BTreeMap<String, (String, Vec<String>)>;

impl Builder<'_, '_> {
    pub(super) fn produced(&mut self, digests: &Digests) {
        let view = self.view;
        let builds: BTreeSet<&str> = view
            .members(Stage::Build)
            .map(|m| m.row.pipeline_id.as_str())
            .collect();
        let mut latest: HashMap<(String, String), (i64, i64, String)> = HashMap::new();
        for edge in view
            .state
            .edges
            .iter()
            .filter(|e| e.from_kind == RUN && e.to_kind == ARTIFACT && e.relation == PRODUCED)
        {
            let Some((pipeline, run)) = parse_run_node(&edge.from_id) else {
                continue;
            };
            let Some((repository, groups)) = digests.get(&edge.to_id) else {
                continue;
            };
            if !builds.contains(pipeline) || !self.nodes.contains_key(&artifact_id(repository)) {
                continue;
            }
            let key = (pipeline.to_string(), repository.clone());
            if latest.get(&key).is_none_or(|(seen, _, _)| *seen < run) {
                let first = groups
                    .iter()
                    .min()
                    .map_or_else(String::new, |g| self.label_of(g));
                latest.insert(key, (run, edge.id, first));
            }
        }
        for ((pipeline, repository), (run, edge, label)) in latest {
            let from = build_id(&pipeline);
            let mut link = Self::exact(
                &from,
                &artifact_id(&repository),
                copy::why_run_produced(run, &label),
            );
            link.edge_id = Some(edge);
            link.unlink_pipeline_id = Some(pipeline);
            self.link(link);
        }
    }

    fn dashed(&mut self, from: &str, to: &str, open: &OpenSuggestion) {
        self.link(FlowLink {
            confidence: Confidence::Suggested,
            suggestion_id: Some(open.dto.id.clone()),
            ..Self::exact(from, to, open.dto.why.join("; "))
        });
    }

    fn suggest_build(&mut self, open: &OpenSuggestion, members: &BTreeSet<&str>) {
        let view = self.view;
        for id in open
            .pipelines
            .iter()
            .filter(|p| !members.contains(p.as_str()))
        {
            let Some(pipeline) = view.pipeline(id) else {
                continue;
            };
            let build = self.build_node(pipeline);
            if let Some(subject) = view.facts.world.subjects.get(id) {
                for source in &subject.sources {
                    let from = self.source(&source.repo);
                    self.dashed(&from, &build, open);
                }
            }
            let anchor = [
                FlowNodeKind::Artifact,
                FlowNodeKind::Group,
                FlowNodeKind::Target,
            ]
            .iter()
            .find_map(|kind| self.nodes.values().find(|n| n.kind == *kind))
            .map(|n| n.id.clone());
            if let Some(anchor) = anchor {
                self.dashed(&build, &anchor, open);
            }
        }
    }

    fn suggest_sibling(&mut self, open: &OpenSuggestion, members: &BTreeSet<&str>) {
        let view = self.view;
        let groups = view.group_pipelines();
        let anchor = open
            .pipelines
            .iter()
            .filter(|p| members.contains(p.as_str()))
            .find_map(|p| {
                groups
                    .iter()
                    .find(|(_, found)| found.iter().any(|f| &f.id == p))
                    .map(|(group, _)| self.node_of(group))
            });
        for id in open
            .pipelines
            .iter()
            .filter(|p| !members.contains(p.as_str()))
        {
            let Some(pipeline) = view.pipeline(id) else {
                continue;
            };
            let node = format!("sibling:{id}");
            self.put(FlowNode {
                id: node.clone(),
                kind: FlowNodeKind::Target,
                column: FIRST_TARGET_COLUMN,
                row: 0,
                title: pipeline.name.clone(),
                subtitle: None,
                status: Some(pipeline.status.clone()),
                pipeline_id: Some(pipeline.id.clone()),
                count: 1,
            });
            if let Some(anchor) = &anchor {
                self.dashed(anchor, &node, open);
            }
        }
    }

    pub(super) fn suggested(&mut self) {
        let view = self.view;
        let members: BTreeSet<&str> = view
            .state
            .members
            .iter()
            .map(|m| m.row.pipeline_id.as_str())
            .collect();
        for open in view
            .facts
            .suggestions
            .iter()
            .filter(|s| s.applications.contains(&view.state.row.id))
        {
            match open.dto.kind {
                SuggestionKind::Membership => self.suggest_build(open, &members),
                SuggestionKind::SameApplication => self.suggest_sibling(open, &members),
                SuggestionKind::BuiltFrom => {}
            }
        }
    }

    pub(super) fn hints(&mut self, changes: &[ChangeData]) {
        let view = self.view;
        let columns: BTreeMap<&str, u32> = view
            .state
            .groups
            .iter()
            .filter_map(|g| Some((g.key.as_str(), self.nodes.get(&target_id(&g.key))?.column)))
            .collect();
        let mut chosen: BTreeMap<&str, (i64, i64, &str)> = BTreeMap::new();
        for change in changes {
            let times: Vec<(&str, DateTime<Utc>)> = columns
                .keys()
                .filter_map(|g| Some((*g, change.first_deploy(g)?.succeeded_at()?)))
                .collect();
            for (group, at) in &times {
                let earlier = times
                    .iter()
                    .filter(|(other, when)| {
                        other != group && columns[other] == columns[group] && when < at
                    })
                    .max_by_key(|(_, when)| *when);
                let Some((other, when)) = earlier else {
                    continue;
                };
                let minutes = (*at - *when).num_minutes();
                if minutes >= 1 && chosen.get(group).is_none_or(|(id, _, _)| *id < change.id) {
                    chosen.insert(group, (change.id, minutes, other));
                }
            }
        }
        for (group, (_, minutes, other)) in chosen {
            let hint = copy::deployed_after(minutes, &self.label_of(other));
            let to = target_id(group);
            if let Some(link) = self
                .links
                .values_mut()
                .find(|l| l.to == to && l.from.starts_with("artifact:"))
            {
                link.hint = Some(hint);
            }
        }
    }
}
