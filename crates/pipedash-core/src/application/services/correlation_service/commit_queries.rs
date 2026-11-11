#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use pipedash_plugin_api::{
    CommitSha,
    RepoId,
};

use super::copy::{
    self,
    views,
};
use super::page::layout::ordered_groups;
use super::page::live::{
    built_from,
    primary,
    BuiltFrom,
    Live,
};
use super::page::{
    chunked,
    repo_id,
    short,
    View,
};
use super::vocab::REL_CONSUMED;
use super::CorrelationService;
use crate::domain::application::dto::{
    ApplicationRef,
    CommitLocation,
    CommitLocations,
    DeployChanges,
    DeployCommit,
    LocationState,
};
use crate::domain::pipeline::PipelineStatus;
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ArtifactObservationRow,
    SourceObservationRow,
};

const MAX_DEPLOY_COMMITS: usize = 50;
const COMMIT_PARALLELISM: usize = 8;

type Commit = (String, String);

fn commits_of(
    sources: &[SourceObservationRow], artifacts: &[ArtifactObservationRow], built: &BuiltFrom,
    run: i64,
) -> BTreeSet<Commit> {
    let mut found: BTreeSet<Commit> = sources
        .iter()
        .filter(|s| s.run_number == run)
        .filter_map(|s| Some((s.repo.clone(), s.commit_sha.clone()?)))
        .collect();
    let consumed = artifacts
        .iter()
        .filter(|a| a.run_number == run && a.relation == REL_CONSUMED)
        .filter_map(|a| a.digest.as_ref());
    for digest in consumed {
        found.extend(built.get(digest).into_iter().flatten().cloned());
    }
    found
}

impl CorrelationService {
    pub async fn pipeline_applications(
        &self, pipeline_id: &str,
    ) -> DomainResult<Vec<ApplicationRef>> {
        let mut conn = self.store.conn();
        let mut found = Vec::new();
        for member in conn
            .list_application_members_by_pipeline(pipeline_id)
            .await?
        {
            let Some(app) = conn.get_application(member.application_id).await? else {
                continue;
            };
            if app.archived_at.is_none() && !app.hidden {
                found.push(ApplicationRef {
                    id: app.id,
                    name: app.name,
                });
            }
        }
        found.sort_by_key(|app| (app.name.to_lowercase(), app.id));
        Ok(found)
    }

    async fn position(
        &self, commits: &BTreeSet<Commit>, live: Option<&Vec<Live>>,
    ) -> (LocationState, String) {
        let live = live.map_or(&[][..], Vec::as_slice);
        if commits
            .iter()
            .any(|(repo, sha)| live.iter().any(|l| l.is(repo, sha)))
        {
            return (LocationState::Live, copy::location_live());
        }
        let anchor = primary(live);
        for (repo, sha) in commits {
            let Some(base) = anchor.filter(|l| &l.repo == repo) else {
                continue;
            };
            let (Some(id), Some(base), Some(head)) = (
                repo_id(repo),
                CommitSha::parse(&base.sha),
                CommitSha::parse(sha),
            ) else {
                continue;
            };
            match self.compare(&id, &base, &head).await {
                Some(found) if found.ahead > 0 => {
                    return (LocationState::Behind, views::location_behind(found.ahead));
                }
                Some(_) => return (LocationState::Live, copy::location_live()),
                None => {}
            }
        }
        (LocationState::NotDeployed, copy::location_not_deployed())
    }

    pub async fn commit_locations(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<CommitLocations> {
        let mut conn = self.store.conn();
        let built = built_from(&mut conn).await?;
        let sources = conn
            .list_source_observations_by_subject(pipeline_id)
            .await?;
        let artifacts = conn
            .list_artifact_observations_by_subject(pipeline_id)
            .await?;
        let commits = commits_of(&sources, &artifacts, &built, run_number);
        let mut entries = Vec::new();
        if !commits.is_empty() {
            let facts = self.facts(None).await?;
            let mut states: Vec<_> = facts
                .states
                .iter()
                .filter(|s| {
                    !s.row.hidden && s.members.iter().any(|m| m.row.pipeline_id == pipeline_id)
                })
                .collect();
            states.sort_by(|a, b| (&a.row.name, a.row.id).cmp(&(&b.row.name, b.row.id)));
            for state in states {
                let live = View::new(&facts, state).live();
                for group in ordered_groups(state).into_iter().filter(|g| !g.short_lived) {
                    let (found, label) = self.position(&commits, live.get(&group.key)).await;
                    entries.push(CommitLocation {
                        application_id: state.row.id,
                        application_name: state.row.name.clone(),
                        target_label: group.label.clone(),
                        state: found,
                        label,
                    });
                }
            }
        }
        Ok(CommitLocations {
            title: copy::commit_locations_title(),
            entries,
        })
    }

    async fn deploy_commit(&self, repo: &RepoId, sha: &CommitSha) -> DeployCommit {
        let info = self.commit_info(repo, sha.as_str()).await;
        DeployCommit {
            sha: sha.as_str().to_string(),
            short: short(sha.as_str()),
            message: info
                .as_ref()
                .and_then(|i| i.message.lines().next())
                .map(str::to_string),
            authored_at: info.map(|i| i.authored_at),
        }
    }

    pub async fn deploy_changes(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<DeployChanges> {
        let mut conn = self.store.conn();
        let built = built_from(&mut conn).await?;
        let sources = conn
            .list_source_observations_by_subject(pipeline_id)
            .await?;
        let artifacts = conn
            .list_artifact_observations_by_subject(pipeline_id)
            .await?;
        let previous = conn
            .list_run_evidence_by_pipeline(pipeline_id)
            .await?
            .into_iter()
            .filter(|run| {
                run.run_number < run_number && run.status == PipelineStatus::Success.as_str()
            })
            .max_by_key(|run| run.run_number);
        let current = commits_of(&sources, &artifacts, &built, run_number);
        let earlier = previous
            .map(|run| commits_of(&sources, &artifacts, &built, run.run_number))
            .unwrap_or_default();
        let pair = current.iter().find_map(|(repo, head)| {
            let base = earlier.iter().find(|(other, _)| other == repo)?;
            Some((repo, base.1.as_str(), head.as_str()))
        });
        let unavailable = |reason: String| DeployChanges {
            title: copy::deploy_changes_title(),
            commits: Vec::new(),
            unavailable_reason: Some(reason),
        };
        let Some((repo, base, head)) = pair else {
            return Ok(unavailable(views::no_previous_deploy()));
        };
        let (Some(id), Some(base), Some(head)) = (
            repo_id(repo),
            CommitSha::parse(base),
            CommitSha::parse(head),
        ) else {
            return Ok(unavailable(views::changes_unavailable()));
        };
        let Some(found) = self.compare(&id, &base, &head).await else {
            return Ok(unavailable(views::changes_unavailable()));
        };
        let shown = &found.commits[..found.commits.len().min(MAX_DEPLOY_COMMITS)];
        let commits = chunked(shown, COMMIT_PARALLELISM, |sha| {
            self.deploy_commit(&id, sha)
        })
        .await;
        Ok(DeployChanges {
            title: copy::deploy_changes_title(),
            commits,
            unavailable_reason: None,
        })
    }
}
