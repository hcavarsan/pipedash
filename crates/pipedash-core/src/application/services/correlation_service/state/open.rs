use std::collections::{
    BTreeMap,
    BTreeSet,
};

use serde_json::Value;

use super::OpenSuggestion;
use crate::application::services::correlation_service::applications::{
    self,
    Plan,
};
use crate::application::services::correlation_service::copy;
use crate::application::services::correlation_service::suggestions::suggestion_id;
use crate::application::services::correlation_service::vocab::{
    parse_commit_node,
    BUILDS,
    LABEL_UNCONFIRMED,
    PIPELINE,
    SAME_APPLICATION,
    SAME_ARTIFACT_REPO_DIGEST,
    SAME_LABEL,
    SAME_NAME,
    SAME_REPO_PATH,
    TAG_SHA,
};
use crate::application::services::correlation_service::world::World;
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::application::dto::{
    Suggestion,
    SuggestionKind,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    CorrelationConn,
    CorrelationEdgeRow,
};

fn text<'a>(evidence: &'a Value, key: &str) -> &'a str {
    evidence
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn short(value: &str) -> &str {
    let value = value.strip_prefix("sha256:").unwrap_or(value);
    value.get(..12).unwrap_or(value)
}

fn pair_why(rule: &str, evidence: &Value) -> String {
    if rule == SAME_REPO_PATH {
        return copy::why_rule_same_repo_path(text(evidence, "repo"), text(evidence, "path"));
    }
    why_rule(rule, evidence)
}

fn why_rule(rule: &str, evidence: &Value) -> String {
    match rule {
        SAME_ARTIFACT_REPO_DIGEST => copy::why_rule_same_artifact(text(evidence, "repository")),
        SAME_REPO_PATH => {
            copy::why_membership_shared(text(evidence, "repo"), text(evidence, "path"))
        }
        SAME_LABEL => copy::why_rule_same_label(text(evidence, "label")),
        SAME_NAME => copy::why_rule_same_name(text(evidence, "name")),
        LABEL_UNCONFIRMED => {
            copy::why_built_from_label(text(evidence, "source"), short(text(evidence, "revision")))
        }
        TAG_SHA => {
            copy::why_built_from_tag(text(evidence, "repo"), short(text(evidence, "commit")))
        }
        _ => String::new(),
    }
}

fn apps_of(world: &World) -> BTreeMap<&str, Vec<i64>> {
    let live: BTreeSet<i64> = world
        .applications
        .values()
        .filter(|row| row.archived_at.is_none())
        .map(|row| row.id)
        .collect();
    let mut map: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
    for rows in world.members.values() {
        for row in rows.iter().filter(|r| live.contains(&r.application_id)) {
            map.entry(row.pipeline_id.as_str())
                .or_default()
                .push(row.application_id);
        }
    }
    map
}

struct Collector<'a> {
    world: &'a World,
    plan: &'a Plan,
    apps: BTreeMap<&'a str, Vec<i64>>,
    found: Vec<OpenSuggestion>,
}

impl Collector<'_> {
    fn unit_pipeline(&self, unit: &str) -> Option<String> {
        self.world.units.get(unit).cloned()
    }

    fn apps_for(&self, pipeline: &str) -> Vec<i64> {
        self.apps.get(pipeline).cloned().unwrap_or_default()
    }

    fn same_application(&mut self, edges: &[&CorrelationEdgeRow]) {
        let mut pairs: BTreeMap<(&str, &str), Vec<&CorrelationEdgeRow>> = BTreeMap::new();
        for edge in edges {
            pairs
                .entry((edge.from_id.as_str(), edge.to_id.as_str()))
                .or_default()
                .push(edge);
        }
        for ((first, second), rows) in pairs {
            let (Some(a), Some(b)) = (self.unit_pipeline(first), self.unit_pipeline(second)) else {
                continue;
            };
            let id = suggestion_id(SuggestionKind::SameApplication, &[first, second], "");
            if self.plan.rejected.contains(&id)
                || self.plan.unit_app.get(first) == self.plan.unit_app.get(second)
            {
                continue;
            }
            let why: Vec<String> = rows
                .iter()
                .map(|r| {
                    pair_why(
                        &r.rule,
                        &serde_json::from_str(&r.evidence_json).unwrap_or_default(),
                    )
                })
                .collect();
            let rules: BTreeSet<&str> = rows.iter().map(|r| r.rule.as_str()).collect();
            let mut applications = self.apps_for(&a);
            applications.extend(self.apps_for(&b));
            self.found.push(OpenSuggestion {
                dto: Suggestion {
                    id,
                    application_id: applications.first().copied(),
                    kind: SuggestionKind::SameApplication,
                    title: copy::suggest_same_application(
                        &self.world.name_of(&a),
                        &self.world.name_of(&b),
                    ),
                    why,
                    rule_count: rules.len(),
                },
                pipelines: vec![a, b],
                applications,
            });
        }
    }

    fn membership(&mut self, edges: &[&CorrelationEdgeRow]) {
        for edge in edges {
            let Some(unit_pipeline) = self.unit_pipeline(&edge.to_id) else {
                continue;
            };
            let builder = edge.from_id.as_str();
            let id = suggestion_id(SuggestionKind::Membership, &[builder, &edge.to_id], "");
            let applications = self.apps_for(&unit_pipeline);
            let already = applications
                .iter()
                .any(|app| self.apps_for(builder).contains(app));
            if self.plan.rejected.contains(&id) || already {
                continue;
            }
            let evidence = serde_json::from_str(&edge.evidence_json).unwrap_or_default();
            self.found.push(OpenSuggestion {
                dto: Suggestion {
                    id,
                    application_id: applications.first().copied(),
                    kind: SuggestionKind::Membership,
                    title: copy::suggest_membership(
                        &self.world.name_of(builder),
                        &self.world.name_of(&unit_pipeline),
                    ),
                    why: vec![why_rule(&edge.rule, &evidence)],
                    rule_count: 1,
                },
                pipelines: vec![builder.to_string()],
                applications,
            });
        }
    }

    fn built_from(&mut self, edges: &[&CorrelationEdgeRow]) {
        for edge in edges {
            let id = suggestion_id(
                SuggestionKind::BuiltFrom,
                &[&edge.from_id, &edge.to_id],
                &edge.rule,
            );
            if self.plan.rejected.contains(&id) {
                continue;
            }
            let pipelines: Vec<String> = self
                .world
                .unit_subjects()
                .filter(|(_, s)| s.consumed_digests().contains(edge.from_id.as_str()))
                .map(|(_, s)| s.id.clone())
                .collect();
            let applications: Vec<i64> = pipelines.iter().flat_map(|p| self.apps_for(p)).collect();
            let commit = parse_commit_node(&edge.to_id).map_or("", |(_, sha)| sha);
            let evidence = serde_json::from_str(&edge.evidence_json).unwrap_or_default();
            self.found.push(OpenSuggestion {
                dto: Suggestion {
                    id,
                    application_id: applications.first().copied(),
                    kind: SuggestionKind::BuiltFrom,
                    title: copy::suggest_built_from(short(&edge.from_id), short(commit)),
                    why: vec![why_rule(&edge.rule, &evidence)],
                    rule_count: 1,
                },
                pipelines,
                applications,
            });
        }
    }
}

impl CorrelationService {
    pub(in crate::application::services::correlation_service) async fn open_suggestions(
        &self, conn: &mut CorrelationConn, world: &World,
    ) -> DomainResult<Vec<OpenSuggestion>> {
        let plan = applications::plan(world);
        let mut pair_edges = Vec::new();
        let mut path_edges = Vec::new();
        let mut built_edges = Vec::new();
        for rule in [
            SAME_ARTIFACT_REPO_DIGEST,
            SAME_REPO_PATH,
            SAME_LABEL,
            SAME_NAME,
            LABEL_UNCONFIRMED,
            TAG_SHA,
        ] {
            for edge in conn.list_edges_by_rule(rule).await? {
                match (edge.relation.as_str(), edge.from_kind.as_str()) {
                    (SAME_APPLICATION, _) => pair_edges.push(edge),
                    (BUILDS, PIPELINE) => path_edges.push(edge),
                    _ => built_edges.push(edge),
                }
            }
        }
        let mut collector = Collector {
            world,
            plan: &plan,
            apps: apps_of(world),
            found: Vec::new(),
        };
        collector.same_application(&pair_edges.iter().collect::<Vec<_>>());
        collector.membership(&path_edges.iter().collect::<Vec<_>>());
        collector.built_from(&built_edges.iter().collect::<Vec<_>>());
        let mut found = collector.found;
        found.sort_by(|a, b| a.dto.id.cmp(&b.dto.id));
        Ok(found)
    }
}
