#[cfg(test)]
mod tests;

use std::collections::HashSet;

use super::layout::ordered_groups;
use super::Facts;
use crate::application::services::correlation_service::copy;
use crate::application::services::correlation_service::state::ApplicationState;
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::application::board::{
    ApplicationBoard,
    ApplicationBoardQuery,
    BoardApplication,
    BoardEnvironment,
    BoardPipeline,
    BoardTarget,
};
use crate::domain::application::Stage;
use crate::domain::pipeline::Pipeline;
use crate::domain::DomainResult;

fn card(pipeline: &Pipeline) -> BoardPipeline {
    BoardPipeline {
        pipeline_id: pipeline.id.clone(),
        name: pipeline.name.clone(),
        repository: pipeline.repository.clone(),
        status: pipeline.status.clone(),
    }
}

fn sorted(mut cards: Vec<BoardPipeline>) -> Vec<BoardPipeline> {
    cards.sort_by(|a, b| {
        (&a.repository, &a.name, &a.pipeline_id).cmp(&(&b.repository, &b.name, &b.pipeline_id))
    });
    cards
}

fn lane(facts: &Facts, state: &ApplicationState, stage: Stage) -> Vec<BoardPipeline> {
    sorted(
        state
            .members
            .iter()
            .filter(|member| member.row.stage == stage.as_str())
            .filter_map(|member| facts.pipelines.get(&member.row.pipeline_id))
            .map(card)
            .collect(),
    )
}

fn environments(state: &ApplicationState) -> Vec<BoardEnvironment> {
    ordered_groups(state)
        .into_iter()
        .map(|group| BoardEnvironment {
            group_key: group.key.clone(),
            label: group.label.clone(),
            targets: state
                .targets
                .iter()
                .filter(|target| target.group_key == group.key)
                .map(|target| BoardTarget {
                    target_key: target.target_key.clone(),
                    name: target.name.clone(),
                })
                .collect(),
        })
        .collect()
}

fn application(facts: &Facts, state: &ApplicationState) -> BoardApplication {
    let editable = state.capabilities.edit;
    BoardApplication {
        id: state.row.id,
        name: state.row.name.clone(),
        editable,
        notice: (!editable).then(copy::board::config_notice),
        build: lane(facts, state, Stage::Build),
        deploy: lane(facts, state, Stage::Deploy),
        environments: environments(state),
    }
}

fn card_matches(card: &BoardPipeline, needle: &str) -> bool {
    card.name.to_lowercase().contains(needle) || card.repository.to_lowercase().contains(needle)
}

fn application_matches(application: &BoardApplication, needle: &str) -> bool {
    application.name.to_lowercase().contains(needle)
        || application
            .build
            .iter()
            .chain(&application.deploy)
            .any(|card| card_matches(card, needle))
}

impl CorrelationService {
    pub async fn board(&self, query: ApplicationBoardQuery) -> DomainResult<ApplicationBoard> {
        let facts = self.facts(None).await?;
        let needle = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|search| !search.is_empty())
            .map(str::to_lowercase);
        let assigned: HashSet<&str> = facts
            .states
            .iter()
            .flat_map(|state| &state.members)
            .map(|member| member.row.pipeline_id.as_str())
            .collect();
        let unassigned = sorted(
            facts
                .pipelines
                .values()
                .filter(|pipeline| !assigned.contains(pipeline.id.as_str()))
                .map(card)
                .filter(|card| needle.as_deref().is_none_or(|n| card_matches(card, n)))
                .collect(),
        );
        let mut applications: Vec<BoardApplication> = facts
            .states
            .iter()
            .map(|state| application(&facts, state))
            .filter(|app| {
                needle
                    .as_deref()
                    .is_none_or(|n| application_matches(app, n))
            })
            .collect();
        applications.sort_by_key(|app| (app.name.to_lowercase(), app.id));
        Ok(ApplicationBoard {
            unassigned,
            applications,
            copy: copy::board::board_copy(),
        })
    }
}
