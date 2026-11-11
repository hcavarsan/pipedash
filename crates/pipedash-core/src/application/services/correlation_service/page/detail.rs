use std::slice;

use super::flow::build_flow;
use super::layout::ordered_groups;
use super::list::summarize;
use super::{
    Facts,
    View,
};
use crate::application::services::correlation_service::copy;
use crate::application::services::correlation_service::state::ApplicationState;
use crate::application::services::correlation_service::toml::{
    render,
    TomlApplication,
    TomlMember,
    TomlTarget,
};
use crate::application::services::correlation_service::vocab::LIFETIME_SHORT_LIVED;
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::application::dto::{
    ApplicationDetail,
    ApplicationMember,
    ApplicationTarget,
    Suggestion,
};
use crate::domain::application::overrides::{
    ApplicationEdit,
    CreateApplicationInput,
};
use crate::domain::application::{
    IdentityKind,
    MemberSource,
    Stage,
};
use crate::domain::{
    DomainError,
    DomainResult,
};

fn targets_of(state: &ApplicationState) -> Vec<ApplicationTarget> {
    let order: Vec<&str> = ordered_groups(state)
        .into_iter()
        .map(|group| group.key.as_str())
        .collect();
    let mut targets: Vec<ApplicationTarget> = state
        .targets
        .iter()
        .map(|target| ApplicationTarget {
            target_key: target.target_key.clone(),
            name: target.name.clone(),
            group_key: target.group_key.clone(),
            group_label: state
                .groups
                .iter()
                .find(|g| g.key == target.group_key)
                .map_or_else(|| target.name.clone(), |g| g.label.clone()),
            short_lived: target.lifetime == LIFETIME_SHORT_LIVED,
        })
        .collect();
    targets.sort_by_key(|target| {
        order
            .iter()
            .position(|key| *key == target.group_key)
            .unwrap_or(usize::MAX)
    });
    targets
}

fn toml_of(facts: &Facts, state: &ApplicationState) -> String {
    let world = &facts.world;
    let members = state
        .members
        .iter()
        .filter_map(|m| {
            Some(TomlMember {
                provider: world.provider_key(&m.row.pipeline_id)?,
                local_id: world.local_id(&m.row.pipeline_id)?,
                stage: Stage::parse(&m.row.stage)?,
            })
        })
        .collect();
    let targets = state
        .targets
        .iter()
        .map(|target| {
            let (cluster, namespace) = target
                .target_key
                .split_once('|')
                .unwrap_or((&target.target_key, ""));
            TomlTarget {
                cluster,
                namespace: (!namespace.is_empty()).then_some(namespace),
                name: &target.name,
                group_key: &target.group_key,
                group_label: state
                    .groups
                    .iter()
                    .find(|g| g.key == target.group_key)
                    .map_or(&target.name, |g| &g.label),
                short_lived: target.lifetime == LIFETIME_SHORT_LIVED,
            }
        })
        .collect();
    render(&TomlApplication {
        name: &state.row.name,
        hidden: state.row.hidden,
        members,
        targets,
        directions: state
            .directions
            .iter()
            .map(|d| (d.from_group.as_str(), d.to_group.as_str()))
            .collect(),
    })
}

impl CorrelationService {
    async fn members_of(&self, view: &View<'_>) -> DomainResult<Vec<ApplicationMember>> {
        let mut found = Vec::new();
        let mut pipelines = Vec::new();
        for member in &view.state.members {
            let (Some(pipeline), Some(stage), Some(source)) = (
                view.pipeline(&member.row.pipeline_id),
                Stage::parse(&member.row.stage),
                MemberSource::parse(&member.row.source),
            ) else {
                continue;
            };
            pipelines.push(pipeline.clone());
            found.push((stage, source, member.why.clone()));
        }
        self.presenter.pipelines(&mut pipelines).await?;
        Ok(pipelines
            .into_iter()
            .zip(found)
            .map(|(pipeline, (stage, source, why))| ApplicationMember {
                pipeline,
                stage,
                source,
                why,
            })
            .collect())
    }

    pub async fn detail(&self, id: i64) -> DomainResult<ApplicationDetail> {
        let facts = self.facts(Some(id)).await?;
        let state = facts
            .states
            .first()
            .ok_or_else(|| DomainError::NotFound(copy::application_not_found(id)))?;
        let view = View::new(&facts, state);
        let live = view.live();
        let behind = self.behind_of(&facts, slice::from_ref(&live)).await;
        let changes = self.all_changes(id).await?;
        Ok(ApplicationDetail {
            summary: summarize(&facts, state, &live, &behind).summary,
            flow: build_flow(&view, &changes),
            members: self.members_of(&view).await?,
            targets: targets_of(state),
            suggestions: facts
                .suggestions
                .iter()
                .filter(|s| s.applications.contains(&id))
                .map(|s| s.dto.clone())
                .collect(),
            overrides: state.overrides.clone(),
            toml: state.capabilities.edit.then(|| toml_of(&facts, state)),
            config_defined: state.kind == IdentityKind::Config,
            copy: copy::application_copy(),
        })
    }

    pub async fn suggestions(&self) -> DomainResult<Vec<Suggestion>> {
        let mut conn = self.store.conn();
        let world = self.load_world(&mut conn).await?;
        let open = self.open_suggestions(&mut conn, &world).await?;
        Ok(open.into_iter().map(|s| s.dto).collect())
    }

    pub async fn create_application(
        &self, input: CreateApplicationInput, author: Option<String>,
    ) -> DomainResult<ApplicationDetail> {
        let id = self.create_internal(input, author).await?;
        self.detail(id).await
    }

    pub async fn apply_edit(
        &self, id: i64, edit: ApplicationEdit, author: Option<String>,
    ) -> DomainResult<ApplicationDetail> {
        let id = self.edit_internal(id, edit, author).await?;
        self.detail(id).await
    }

    pub async fn revert_override(
        &self, application_id: i64, override_id: i64,
    ) -> DomainResult<ApplicationDetail> {
        let id = self.revert_internal(application_id, override_id).await?;
        self.detail(id).await
    }
}
