mod changes;
mod detail;
mod flow;
pub(super) mod layout;
mod list;
pub(super) mod live;
#[cfg(test)]
mod tests;

use std::collections::{
    BTreeMap,
    HashMap,
};
use std::future::Future;

use futures::future::join_all;
use pipedash_plugin_api::RepoId;

use self::live::{
    live_commits,
    BuiltFrom,
    Live,
};
use super::state::{
    ApplicationState,
    MemberState,
    OpenSuggestion,
};
use super::world::{
    Subject,
    World,
};
use super::CorrelationService;
use crate::domain::application::Stage;
use crate::domain::pipeline::{
    Pipeline,
    PipelineStatus,
};
use crate::domain::DomainResult;

pub(super) struct Facts {
    pub world: World,
    pub states: Vec<ApplicationState>,
    pub pipelines: HashMap<String, Pipeline>,
    pub built: BuiltFrom,
    pub suggestions: Vec<OpenSuggestion>,
}

pub(super) struct View<'a> {
    pub facts: &'a Facts,
    pub state: &'a ApplicationState,
}

pub(super) fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

pub(super) async fn chunked<'a, I, T, Fut>(
    items: &'a [I], size: usize, f: impl Fn(&'a I) -> Fut,
) -> Vec<T>
where
    Fut: Future<Output = T>,
{
    let mut out = Vec::with_capacity(items.len());
    for chunk in items.chunks(size) {
        out.extend(join_all(chunk.iter().map(&f)).await);
    }
    out
}

pub(super) fn repo_id(stored: &str) -> Option<RepoId> {
    let (host, path) = stored.split_once('/')?;
    RepoId::from_host_path(host, path)
}

pub(super) fn badness(status: &PipelineStatus) -> u8 {
    match status {
        PipelineStatus::Success => 0,
        PipelineStatus::Skipped => 1,
        PipelineStatus::Cancelled => 2,
        PipelineStatus::Pending => 3,
        PipelineStatus::Running => 4,
        PipelineStatus::Degraded => 5,
        PipelineStatus::Failed => 6,
    }
}

pub(super) fn worst<'a>(pipelines: &[&'a Pipeline]) -> Option<&'a Pipeline> {
    pipelines
        .iter()
        .copied()
        .max_by(|a, b| (badness(&a.status), &b.id).cmp(&(badness(&b.status), &a.id)))
}

pub(super) fn unit_groups<'a>(
    group_of: &HashMap<&'a str, &'a str>, subject: &Subject, ordinal: Option<usize>,
) -> Vec<&'a str> {
    subject
        .targets
        .iter()
        .enumerate()
        .filter(|(index, _)| ordinal.is_none_or(|o| o == *index))
        .filter_map(|(_, target)| group_of.get(target.key().as_str()).copied())
        .collect()
}

impl<'a> View<'a> {
    pub(super) fn new(facts: &'a Facts, state: &'a ApplicationState) -> Self {
        Self { facts, state }
    }

    pub(super) fn pipeline(&self, id: &str) -> Option<&'a Pipeline> {
        self.facts.pipelines.get(id)
    }

    pub(super) fn members(&self, stage: Stage) -> impl Iterator<Item = &'a MemberState> {
        self.state
            .members
            .iter()
            .filter(move |m| m.row.stage == stage.as_str())
    }

    pub(super) fn group_of(&self) -> HashMap<&'a str, &'a str> {
        self.state
            .targets
            .iter()
            .map(|t| (t.target_key.as_str(), t.group_key.as_str()))
            .collect()
    }

    pub(super) fn group_pipelines(&self) -> BTreeMap<&'a str, Vec<&'a Pipeline>> {
        let group_of = self.group_of();
        let mut found: BTreeMap<&str, Vec<&Pipeline>> = BTreeMap::new();
        for member in self.members(Stage::Deploy) {
            let (Some(subject), Some(pipeline)) = (
                self.facts.world.subjects.get(&member.row.pipeline_id),
                self.pipeline(&member.row.pipeline_id),
            ) else {
                continue;
            };
            let mut groups = unit_groups(&group_of, subject, None);
            groups.sort_unstable();
            groups.dedup();
            for group in groups {
                found.entry(group).or_default().push(pipeline);
            }
        }
        found
    }

    pub(super) fn live(&self) -> BTreeMap<String, Vec<Live>> {
        live_commits(
            &self.facts.world,
            &self.facts.built,
            &self.state.members,
            &self.state.targets,
        )
    }
}

impl CorrelationService {
    pub(super) async fn facts(&self, only: Option<i64>) -> DomainResult<Facts> {
        let mut conn = self.store.conn();
        let world = self.load_world(&mut conn).await?;
        let states = self.states_in(&mut conn, &world, only).await?;
        let suggestions = self.open_suggestions(&mut conn, &world).await?;
        let built = live::built_from(&mut conn).await?;
        let pipelines = self
            .repository
            .get_cached_pipelines(None)
            .await?
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        Ok(Facts {
            world,
            states,
            pipelines,
            built,
            suggestions,
        })
    }
}
