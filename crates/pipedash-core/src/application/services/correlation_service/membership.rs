use std::collections::{
    BTreeMap,
    BTreeSet,
};

use serde_json::json;

use super::applications::{
    Plan,
    PlannedApp,
};
use super::edges::EdgeSpec;
use super::hubs::{
    repo_entity,
    Hubs,
};
use super::suggestions::suggestion_id;
use super::vocab::{
    parse_run_node,
    ARTIFACT,
    BUILDS,
    PIPELINE,
    PRODUCED,
    RUN,
    SAME_REPO_PATH,
    UNIT,
};
use super::world::{
    SourceRef,
    Subject,
    World,
};
use super::CorrelationService;
use crate::domain::application::dto::SuggestionKind;
use crate::domain::application::{
    Confidence,
    MemberSource,
    Stage,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ApplicationMemberRow,
    CorrelationConn,
};

#[cfg(test)]
mod tests;

pub(super) type Builders<'a> = BTreeMap<&'a str, Vec<&'a Subject>>;

pub(super) struct Members {
    pub rows: Vec<ApplicationMemberRow>,
    pub suggestions: Vec<EdgeSpec>,
}

struct Found {
    source: MemberSource,
    edge_id: Option<i64>,
    stage: Option<Stage>,
}

pub(super) fn builder_index(world: &World) -> Builders<'_> {
    let mut index: Builders = BTreeMap::new();
    for builder in world.builders() {
        let repos: BTreeSet<&str> = builder.sources.iter().map(|s| s.repo.as_str()).collect();
        for repo in repos {
            index.entry(repo).or_default().push(builder);
        }
    }
    index
}

pub(super) fn derived_stage(world: &World, pipeline: &str) -> Stage {
    match world.subjects.get(pipeline) {
        Some(subject) if !subject.is_build() => Stage::Deploy,
        _ => Stage::Build,
    }
}

struct Overlap {
    exact: Vec<String>,
    suggested: Vec<(String, String, String, String)>,
}

fn overlaps(world: &World, builders: &Builders, app: &PlannedApp) -> Overlap {
    let mut overlap = Overlap {
        exact: Vec::new(),
        suggested: Vec::new(),
    };
    for unit in &app.units {
        let Some(subject) = world.units.get(unit).and_then(|id| world.subjects.get(id)) else {
            continue;
        };
        for unit_source in &subject.sources {
            let mut matching: Vec<(&Subject, &SourceRef)> = Vec::new();
            for builder in builders
                .get(unit_source.repo.as_str())
                .into_iter()
                .flatten()
            {
                for source in builder.sources.iter().filter(|s| s.overlaps(unit_source)) {
                    matching.push((builder, source));
                }
            }
            let distinct: BTreeSet<&str> = matching.iter().map(|(b, _)| b.id.as_str()).collect();
            for (builder, source) in matching {
                if distinct.len() == 1 || !source.scopes.is_empty() {
                    overlap.exact.push(builder.id.clone());
                } else {
                    overlap.suggested.push((
                        builder.id.clone(),
                        unit.clone(),
                        unit_source.repo.clone(),
                        unit_source.path().to_string(),
                    ));
                }
            }
        }
    }
    overlap
}

impl CorrelationService {
    async fn producers(
        &self, conn: &mut CorrelationConn, world: &World, app: &PlannedApp,
    ) -> DomainResult<BTreeMap<String, i64>> {
        let mut found: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        for unit in &app.units {
            let Some(subject) = world.units.get(unit).and_then(|id| world.subjects.get(id)) else {
                continue;
            };
            for digest in subject.consumed_digests() {
                for edge in conn.list_edges_to(ARTIFACT, digest).await? {
                    if edge.relation != PRODUCED || edge.from_kind != RUN {
                        continue;
                    }
                    let Some((pipeline, run)) = parse_run_node(&edge.from_id) else {
                        continue;
                    };
                    if world.subjects.get(pipeline).is_some_and(|s| !s.is_build()) {
                        continue;
                    }
                    let newer = match found.get(pipeline) {
                        Some((best, _)) => run > *best,
                        None => true,
                    };
                    if newer {
                        found.insert(pipeline.to_string(), (run, edge.id));
                    }
                }
            }
        }
        Ok(found.into_iter().map(|(p, (_, edge))| (p, edge)).collect())
    }

    pub(super) async fn plan_members(
        &self, conn: &mut CorrelationConn, world: &World, plan: &Plan, hubs: &Hubs,
        builders: &Builders<'_>, app: &PlannedApp,
    ) -> DomainResult<Members> {
        let encoded = app.identity.encode();
        let linked = |pipeline: &str| {
            plan.unlinked
                .contains(&(encoded.clone(), pipeline.to_string()))
        };
        let mut found: BTreeMap<String, Found> = BTreeMap::new();
        for unit in &app.units {
            if let Some(id) = world.units.get(unit) {
                let source = plan.unit_source.get(unit).copied();
                found.insert(
                    id.clone(),
                    Found {
                        source: source.unwrap_or(MemberSource::Exact),
                        edge_id: None,
                        stage: None,
                    },
                );
            }
        }
        for (pipeline, edge) in self.producers(conn, world, app).await? {
            if !found.contains_key(&pipeline) && !linked(&pipeline) {
                found.insert(
                    pipeline,
                    Found {
                        source: MemberSource::Exact,
                        edge_id: Some(edge),
                        stage: None,
                    },
                );
            }
        }
        let overlap = overlaps(world, builders, app);
        for pipeline in overlap.exact {
            if !found.contains_key(&pipeline) && !linked(&pipeline) {
                found.insert(
                    pipeline,
                    Found {
                        source: MemberSource::Exact,
                        edge_id: None,
                        stage: None,
                    },
                );
            }
        }
        let mut suggestions = Vec::new();
        for (builder, unit, repo, path) in overlap.suggested {
            let id = suggestion_id(SuggestionKind::Membership, &[&builder, &unit], "");
            let blocked = found.contains_key(&builder)
                || linked(&builder)
                || hubs.is_hub(&repo_entity(&repo))
                || plan.rejected.contains(&id);
            if blocked {
                continue;
            }
            suggestions.push(EdgeSpec {
                from_kind: PIPELINE,
                from_id: builder,
                to_kind: UNIT,
                to_id: unit,
                relation: BUILDS,
                confidence: Confidence::Suggested,
                rule: SAME_REPO_PATH,
                evidence: json!({ "repo": repo, "path": path }),
            });
        }
        for (pipeline, explicit) in &app.explicit {
            let entry = found.entry(pipeline.clone()).or_insert(Found {
                source: explicit.source,
                edge_id: None,
                stage: None,
            });
            entry.source = explicit.source;
            entry.stage = explicit.stage;
        }
        for pipeline in &app.excluded {
            found.remove(pipeline);
        }
        let rows = found
            .into_iter()
            .map(|(pipeline, member)| {
                let stage = plan
                    .stages
                    .get(&(encoded.clone(), pipeline.clone()))
                    .copied()
                    .or(member.stage)
                    .unwrap_or_else(|| derived_stage(world, &pipeline));
                ApplicationMemberRow {
                    application_id: 0,
                    pipeline_id: pipeline,
                    stage: stage.as_str().to_string(),
                    source: member.source.as_str().to_string(),
                    edge_id: member.edge_id,
                }
            })
            .collect();
        Ok(Members { rows, suggestions })
    }
}
