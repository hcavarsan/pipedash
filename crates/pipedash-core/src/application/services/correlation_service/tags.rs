use std::collections::{
    BTreeMap,
    BTreeSet,
};

use pipedash_plugin_api::{
    CommitSha,
    RepoId,
};
use regex::Regex;

use super::world::{
    Affected,
    Subject,
    World,
};
use super::CorrelationService;

#[cfg(test)]
mod tests;

const TAG_PATTERN: &str = r"(?i)(?:^|[^0-9a-f])([0-9a-f]{7,40})(?:$|[^0-9a-f])";

pub(super) fn has_candidates(world: &World, affected: &Affected) -> bool {
    let Ok(pattern) = Regex::new(TAG_PATTERN) else {
        return false;
    };
    world
        .subjects
        .values()
        .filter(|s| affected.has(&s.id))
        .flat_map(|s| s.consumed.iter().chain(&s.produced))
        .any(|a| a.digest.is_some() && a.tag.as_deref().is_some_and(|t| pattern.is_match(t)))
}

pub(super) type TagEdges = BTreeMap<String, Option<(String, String)>>;

fn candidate_repos(world: &World, digest: &str, subject: &Subject) -> BTreeSet<String> {
    if let Some(source) = world
        .artifacts
        .get(digest)
        .and_then(|a| a.label_source.clone())
    {
        return BTreeSet::from([source]);
    }
    subject.sources.iter().map(|s| s.repo.clone()).collect()
}

impl CorrelationService {
    pub(super) async fn resolve_unique_commit(
        &self, repo: &RepoId, prefix: &str,
    ) -> Option<CommitSha> {
        let info = self.commit_info(repo, prefix).await?;
        info.sha.as_str().starts_with(prefix).then_some(info.sha)
    }

    async fn unique_in_repos(
        &self, repos: &BTreeSet<String>, prefix: &str,
    ) -> Option<(String, String)> {
        let mut found = BTreeSet::new();
        for repo in repos {
            let Some((host, path)) = repo.split_once('/') else {
                continue;
            };
            let Some(id) = RepoId::from_host_path(host, path) else {
                continue;
            };
            if let Some(sha) = self.resolve_unique_commit(&id, prefix).await {
                found.insert((repo.clone(), sha.as_str().to_string()));
            }
        }
        let mut all = found.into_iter();
        let first = all.next()?;
        all.next().is_none().then_some(first)
    }

    pub(super) async fn tag_edges(&self, world: &World, affected: &Affected) -> TagEdges {
        let mut edges = TagEdges::new();
        let Ok(pattern) = Regex::new(TAG_PATTERN) else {
            return edges;
        };
        for subject in world.subjects.values().filter(|s| affected.has(&s.id)) {
            for artifact in subject.consumed.iter().chain(&subject.produced) {
                let (Some(digest), Some(tag)) = (&artifact.digest, &artifact.tag) else {
                    continue;
                };
                if edges.contains_key(digest) {
                    continue;
                }
                let Some(prefix) = pattern
                    .captures(tag)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_lowercase())
                else {
                    continue;
                };
                let repos = candidate_repos(world, digest, subject);
                let found = self.unique_in_repos(&repos, &prefix).await;
                edges.insert(digest.clone(), found);
            }
        }
        edges
    }
}
