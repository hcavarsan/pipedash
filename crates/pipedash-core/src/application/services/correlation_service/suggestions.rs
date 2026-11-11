use std::collections::{
    BTreeMap,
    BTreeSet,
};

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::CommitSha;
use serde_json::{
    json,
    Value,
};
use sha2::{
    Digest as _,
    Sha256,
};

use super::applications::Plan;
use super::edges::{
    apply,
    EdgeSpec,
};
use super::hubs::{
    artifact_repository_entity,
    digest_entity,
    label_entity,
    name_entity,
    repo_entity,
    Hubs,
};
use super::names::name_key;
use super::tags::TagEdges;
use super::vocab::{
    commit_node_id,
    ARTIFACT,
    BUILT_FROM,
    COMMIT,
    LABEL_CONFIRMED,
    LABEL_UNCONFIRMED,
    PRODUCED,
    RUN,
    SAME_APPLICATION,
    SAME_ARTIFACT_REPO_DIGEST,
    SAME_LABEL,
    SAME_NAME,
    SAME_REPO_PATH,
    TAG_SHA,
    UNIT,
};
use super::world::{
    Affected,
    World,
};
use super::CorrelationService;
use crate::domain::application::dto::SuggestionKind;
use crate::domain::application::Confidence;
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    CorrelationConn,
    CorrelationEdgeRow,
    PIPELINE_LEVEL,
};

#[cfg(test)]
mod tests;

const PAIR_RULES: [&str; 4] = [
    SAME_ARTIFACT_REPO_DIGEST,
    SAME_REPO_PATH,
    SAME_LABEL,
    SAME_NAME,
];

pub(super) fn suggestion_id(kind: SuggestionKind, subjects: &[&str], rule: &str) -> String {
    let mut sorted = subjects.to_vec();
    sorted.sort_unstable();
    let kind = match kind {
        SuggestionKind::SameApplication => "same_application",
        SuggestionKind::Membership => "membership",
        SuggestionKind::BuiltFrom => "built_from",
    };
    let hash = Sha256::digest(format!("{kind}|{}|{rule}", sorted.join(",")).as_bytes());
    hash.iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

type Pairs = BTreeMap<(String, String, &'static str), Value>;

struct Pairing<'a> {
    world: &'a World,
    hubs: &'a Hubs,
    plan: &'a Plan,
    affected: &'a Affected,
    found: Pairs,
}

impl Pairing<'_> {
    fn emit(
        &mut self, rule: &'static str, entities: &[String], units: &BTreeSet<&str>,
        evidence: impl Fn(&str, &str) -> Option<Value>,
    ) {
        if units.len() < 2 || entities.iter().any(|e| self.hubs.is_hub(e)) {
            return;
        }
        let units: Vec<&str> = units.iter().copied().collect();
        for (index, first) in units.iter().enumerate() {
            for second in &units[index + 1..] {
                if self.skip(first, second) {
                    continue;
                }
                if let Some(value) = evidence(first, second) {
                    self.found
                        .insert((first.to_string(), second.to_string(), rule), value);
                }
            }
        }
    }

    fn skip(&self, first: &str, second: &str) -> bool {
        let touched = |unit: &str| {
            self.world
                .units
                .get(unit)
                .is_some_and(|id| self.affected.has(id))
        };
        if !touched(first) && !touched(second) {
            return true;
        }
        if self.plan.unit_app.get(first) == self.plan.unit_app.get(second) {
            return true;
        }
        let id = suggestion_id(SuggestionKind::SameApplication, &[first, second], "");
        self.plan.rejected.contains(&id)
    }
}

pub(super) fn pair_edges(
    world: &World, hubs: &Hubs, plan: &Plan, affected: &Affected,
) -> Vec<EdgeSpec> {
    let mut digests: BTreeMap<(String, String), BTreeSet<&str>> = BTreeMap::new();
    let mut repos: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut labels: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut names: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (unit, subject) in world.unit_subjects() {
        for artifact in &subject.consumed {
            if let Some(digest) = &artifact.digest {
                digests
                    .entry((artifact.repository.clone(), digest.clone()))
                    .or_default()
                    .insert(unit);
            }
        }
        for source in &subject.sources {
            repos.entry(source.repo.clone()).or_default().insert(unit);
        }
        if let Some(label) = &subject.label {
            labels.entry(label.clone()).or_default().insert(unit);
        }
        if let Some(key) = name_key(&world.name_of(&subject.id)) {
            names.entry(key).or_default().insert(unit);
        }
    }
    let mut pairing = Pairing {
        world,
        hubs,
        plan,
        affected,
        found: Pairs::new(),
    };
    for ((repository, digest), units) in &digests {
        let entities = [
            digest_entity(digest),
            artifact_repository_entity(repository),
        ];
        let evidence = json!({ "repository": repository, "digest": digest });
        pairing.emit(SAME_ARTIFACT_REPO_DIGEST, &entities, units, |_, _| {
            Some(evidence.clone())
        });
    }
    for (repo, units) in &repos {
        let overlap = |a: &str, b: &str| {
            let (first, second) = (unit_sources(world, a, repo), unit_sources(world, b, repo));
            first
                .iter()
                .find(|x| second.iter().any(|y| x.overlaps(y)))
                .map(|x| json!({ "repo": repo, "path": x.path() }))
        };
        pairing.emit(SAME_REPO_PATH, &[repo_entity(repo)], units, overlap);
    }
    for (label, units) in &labels {
        let evidence = json!({ "label": label });
        pairing.emit(SAME_LABEL, &[label_entity(label)], units, |_, _| {
            Some(evidence.clone())
        });
    }
    for (name, units) in &names {
        let evidence = json!({ "name": name });
        pairing.emit(SAME_NAME, &[name_entity(name)], units, |_, _| {
            Some(evidence.clone())
        });
    }
    pairing
        .found
        .into_iter()
        .map(|((from, to, rule), evidence)| EdgeSpec {
            from_kind: UNIT,
            from_id: from,
            to_kind: UNIT,
            to_id: to,
            relation: SAME_APPLICATION,
            confidence: Confidence::Suggested,
            rule,
            evidence,
        })
        .collect()
}

fn unit_sources<'a>(world: &'a World, unit: &str, repo: &str) -> Vec<&'a super::world::SourceRef> {
    world
        .units
        .get(unit)
        .and_then(|id| world.subjects.get(id))
        .map(|s| s.sources.iter().filter(|r| r.repo == repo).collect())
        .unwrap_or_default()
}

impl CorrelationService {
    pub(super) async fn sync_pair_edges(
        &self, conn: &mut CorrelationConn, world: &World, plan: &Plan, hubs: &Hubs,
        affected: &Affected, now: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let mut existing: BTreeMap<i64, CorrelationEdgeRow> = BTreeMap::new();
        for (unit, subject) in world.unit_subjects() {
            if !affected.has(&subject.id) {
                continue;
            }
            for row in conn.list_edges_from(UNIT, unit).await? {
                existing.insert(row.id, row);
            }
            for row in conn.list_edges_to(UNIT, unit).await? {
                existing.insert(row.id, row);
            }
        }
        let existing: Vec<CorrelationEdgeRow> = existing
            .into_values()
            .filter(|row| row.relation == SAME_APPLICATION)
            .collect();
        let desired = pair_edges(world, hubs, plan, affected);
        let applied = apply(
            conn,
            existing,
            &desired,
            |row| PAIR_RULES.contains(&row.rule.as_str()),
            now,
        )
        .await?;
        Ok(applied.changed)
    }

    async fn has_producer(&self, conn: &mut CorrelationConn, digest: &str) -> DomainResult<bool> {
        let edges = conn.list_edges_to(ARTIFACT, digest).await?;
        Ok(edges
            .iter()
            .any(|e| e.relation == PRODUCED && e.from_kind == RUN))
    }

    async fn affected_digests(
        &self, conn: &mut CorrelationConn, world: &World, affected: &Affected,
    ) -> DomainResult<BTreeSet<String>> {
        if affected.all {
            return Ok(world.artifacts.keys().cloned().collect());
        }
        let mut digests = BTreeSet::new();
        for id in &affected.subjects {
            if let Some(subject) = world.subjects.get(id) {
                digests.extend(
                    subject
                        .consumed
                        .iter()
                        .chain(&subject.produced)
                        .filter_map(|a| a.digest.clone()),
                );
            }
            for row in conn.list_artifact_observations_by_subject(id).await? {
                if row.run_number != PIPELINE_LEVEL {
                    digests.extend(row.digest);
                }
            }
        }
        Ok(digests)
    }

    pub(super) async fn sync_artifact_edges(
        &self, conn: &mut CorrelationConn, world: &World, plan: &Plan, affected: &Affected,
        tags: Option<&TagEdges>, now: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let mut changed = false;
        for digest in self.affected_digests(conn, world, affected).await? {
            let mut desired = Vec::new();
            let label = world.artifacts.get(&digest).and_then(|a| {
                let source = a.label_source.as_ref()?;
                let revision = CommitSha::parse(a.label_revision.as_ref()?)?;
                Some((source.clone(), revision.as_str().to_string()))
            });
            if let Some((source, revision)) = label {
                let confirmed = self.has_producer(conn, &digest).await?;
                let (rule, confidence) = if confirmed {
                    (LABEL_CONFIRMED, Confidence::Exact)
                } else {
                    (LABEL_UNCONFIRMED, Confidence::Suggested)
                };
                desired.push(built_from(
                    &digest,
                    &source,
                    &revision,
                    rule,
                    confidence,
                    json!({
                        "source": source,
                        "revision": revision,
                    }),
                ));
            }
            let tagged = tags.and_then(|t| t.get(&digest)).and_then(Option::as_ref);
            if let Some((repo, sha)) = tagged {
                desired.push(built_from(
                    &digest,
                    repo,
                    sha,
                    TAG_SHA,
                    Confidence::Suggested,
                    json!({
                        "repo": repo,
                        "commit": sha,
                    }),
                ));
            }
            desired.retain(|spec| {
                spec.confidence == Confidence::Exact || {
                    let id = suggestion_id(
                        SuggestionKind::BuiltFrom,
                        &[&spec.from_id, &spec.to_id],
                        spec.rule,
                    );
                    !plan.rejected.contains(&id)
                }
            });
            let existing = conn.list_edges_from(ARTIFACT, &digest).await?;
            let cleanup_tags = tags.is_some();
            let applied = apply(
                conn,
                existing,
                &desired,
                |row| {
                    row.rule == LABEL_CONFIRMED
                        || row.rule == LABEL_UNCONFIRMED
                        || (cleanup_tags && row.rule == TAG_SHA)
                },
                now,
            )
            .await?;
            changed |= applied.changed;
        }
        Ok(changed)
    }
}

fn built_from(
    digest: &str, repo: &str, sha: &str, rule: &'static str, confidence: Confidence,
    evidence: Value,
) -> EdgeSpec {
    EdgeSpec {
        from_kind: ARTIFACT,
        from_id: digest.to_string(),
        to_kind: COMMIT,
        to_id: commit_node_id(repo, sha),
        relation: BUILT_FROM,
        confidence,
        rule,
        evidence,
    }
}
