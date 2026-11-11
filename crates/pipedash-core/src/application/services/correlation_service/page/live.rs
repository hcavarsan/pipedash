use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
};

use super::unit_groups;
use crate::application::services::correlation_service::state::MemberState;
use crate::application::services::correlation_service::vocab::{
    parse_commit_node,
    ARTIFACT,
    ATTESTATION_COMMIT,
    BUILT_FROM,
    LABEL_CONFIRMED,
};
use crate::application::services::correlation_service::world::World;
use crate::domain::application::{
    Confidence,
    Stage,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ApplicationTargetRow,
    CorrelationConn,
};

pub(in crate::application::services::correlation_service) type BuiltFrom =
    HashMap<String, Vec<(String, String)>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::application::services::correlation_service) struct Live {
    pub repo: String,
    pub sha: String,
    pub derived: bool,
}

impl Live {
    pub(in crate::application::services::correlation_service) fn is(
        &self, repo: &str, sha: &str,
    ) -> bool {
        self.repo == repo && self.sha == sha
    }
}

pub(in crate::application::services::correlation_service) fn primary(
    live: &[Live],
) -> Option<&Live> {
    live.iter().find(|l| l.derived).or_else(|| live.first())
}

pub(in crate::application::services::correlation_service) async fn built_from(
    conn: &mut CorrelationConn,
) -> DomainResult<BuiltFrom> {
    let mut found: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    for rule in [ATTESTATION_COMMIT, LABEL_CONFIRMED] {
        for edge in conn.list_edges_by_rule(rule).await? {
            if edge.from_kind != ARTIFACT
                || edge.relation != BUILT_FROM
                || edge.confidence != Confidence::Exact.as_str()
            {
                continue;
            }
            if let Some((repo, sha)) = parse_commit_node(&edge.to_id) {
                found
                    .entry(edge.from_id)
                    .or_default()
                    .insert((repo.to_string(), sha.to_string()));
            }
        }
    }
    Ok(found
        .into_iter()
        .map(|(digest, commits)| (digest, commits.into_iter().collect()))
        .collect())
}

pub(in crate::application::services::correlation_service) fn live_commits(
    world: &World, built: &BuiltFrom, members: &[MemberState], targets: &[ApplicationTargetRow],
) -> BTreeMap<String, Vec<Live>> {
    let group_of: HashMap<&str, &str> = targets
        .iter()
        .map(|t| (t.target_key.as_str(), t.group_key.as_str()))
        .collect();
    let mut out: BTreeMap<String, BTreeSet<Live>> = BTreeMap::new();
    let deploys = members
        .iter()
        .map(|m| &m.row)
        .filter(|row| row.stage == Stage::Deploy.as_str());
    for member in deploys {
        let Some(subject) = world.subjects.get(&member.pipeline_id) else {
            continue;
        };
        for source in &subject.sources {
            let Some(sha) = &source.commit else {
                continue;
            };
            for group in unit_groups(&group_of, subject, None) {
                out.entry(group.to_string()).or_default().insert(Live {
                    repo: source.repo.clone(),
                    sha: sha.clone(),
                    derived: false,
                });
            }
        }
        for artifact in &subject.consumed {
            let Some(digest) = &artifact.digest else {
                continue;
            };
            for (repo, sha) in built.get(digest).into_iter().flatten() {
                for group in unit_groups(&group_of, subject, artifact.target) {
                    out.entry(group.to_string()).or_default().insert(Live {
                        repo: repo.clone(),
                        sha: sha.clone(),
                        derived: true,
                    });
                }
            }
        }
    }
    out.into_iter()
        .map(|(group, live)| (group, live.into_iter().collect()))
        .collect()
}
