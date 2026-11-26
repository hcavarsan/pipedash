mod drop;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use super::layout::ordered_groups;
use super::Facts;
use crate::application::services::correlation_service::copy::application_copy;
use crate::application::services::correlation_service::copy::board as copy;
use crate::application::services::correlation_service::state::ApplicationState;
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::application::board::{
    ApplicationBoard,
    ApplicationBoardQuery,
    BoardApplication,
    BoardCard,
    BoardCardKind,
    BoardCardRef,
    BoardEnvironment,
    BoardLane,
    BoardPipeline,
    BoardTarget,
    BoardZone,
    BoardZoneKind,
    BoardZoneRef,
};
use crate::domain::application::Stage;
use crate::domain::pipeline::Pipeline;
use crate::domain::DomainResult;

const PIPELINES: &str = "pipeline";
const UNASSIGNED_ZONE: &str = "unassigned";

fn targets_class(application: i64) -> String {
    format!("target:{application}")
}

struct Owner<'a> {
    id: i64,
    name: &'a str,
    editable: bool,
}

fn zone(
    kind: BoardZoneKind, owner: Option<&Owner<'_>>, key: Option<&str>, label: String,
    hint: Option<String>,
) -> BoardZone {
    let application_id = owner.map(|o| o.id);
    let scope = application_id.map_or_else(String::new, |id| format!(":{id}"));
    let suffix = key.map_or_else(String::new, |key| format!(":{key}"));
    let name = match kind {
        BoardZoneKind::Unassigned => UNASSIGNED_ZONE,
        BoardZoneKind::NewApplication => "new_application",
        BoardZoneKind::Lane => "lane",
        BoardZoneKind::Environment => "environment",
        BoardZoneKind::NewEnvironment => "new_environment",
    };
    let targets = matches!(
        kind,
        BoardZoneKind::Environment | BoardZoneKind::NewEnvironment
    );
    BoardZone {
        id: format!("{name}{scope}{suffix}"),
        target: BoardZoneRef {
            kind,
            application_id,
            key: key.map(str::to_string),
        },
        drop_class: match application_id.filter(|_| targets) {
            Some(id) => targets_class(id),
            None => PIPELINES.to_string(),
        },
        enabled: owner.is_none_or(|o| o.editable),
        label,
        hint,
    }
}

fn pipeline_card(pipeline: &Pipeline, owner: Option<&Owner<'_>>, zone_id: &str) -> BoardPipeline {
    BoardPipeline {
        card: BoardCard {
            item: BoardCardRef {
                kind: BoardCardKind::Pipeline,
                key: pipeline.id.clone(),
                application_id: owner.map(|o| o.id),
            },
            zone_id: zone_id.to_string(),
            drop_class: PIPELINES.to_string(),
            draggable: owner.is_none_or(|o| o.editable),
        },
        name: pipeline.name.clone(),
        repository: pipeline.repository.clone(),
        status: pipeline.status.clone(),
    }
}

fn sorted(mut cards: Vec<BoardPipeline>) -> Vec<BoardPipeline> {
    cards.sort_by(|a, b| {
        (&a.repository, &a.name, &a.card.item.key).cmp(&(&b.repository, &b.name, &b.card.item.key))
    });
    cards
}

fn lane(facts: &Facts, state: &ApplicationState, owner: &Owner<'_>, stage: Stage) -> BoardLane {
    let title = match stage {
        Stage::Build => application_copy().stage_build,
        Stage::Deploy => application_copy().stage_deploy,
    };
    let members: Vec<&Pipeline> = state
        .members
        .iter()
        .filter(|member| member.row.stage == stage.as_str())
        .filter_map(|member| facts.pipelines.get(&member.row.pipeline_id))
        .collect();
    let hint = (members.is_empty() && owner.editable).then(copy::lane_hint);
    let zone = zone(
        BoardZoneKind::Lane,
        Some(owner),
        Some(stage.as_str()),
        copy::zone_label(owner.name, &title),
        hint,
    );
    let pipelines = sorted(
        members
            .into_iter()
            .map(|pipeline| pipeline_card(pipeline, Some(owner), &zone.id))
            .collect(),
    );
    BoardLane {
        zone,
        title,
        pipelines,
    }
}

fn environments(state: &ApplicationState, owner: &Owner<'_>) -> Vec<BoardEnvironment> {
    ordered_groups(state)
        .into_iter()
        .map(|group| {
            let zone = zone(
                BoardZoneKind::Environment,
                Some(owner),
                Some(&group.key),
                copy::zone_label(owner.name, &group.label),
                None,
            );
            let targets = state
                .targets
                .iter()
                .filter(|target| target.group_key == group.key)
                .map(|target| BoardTarget {
                    card: BoardCard {
                        item: BoardCardRef {
                            kind: BoardCardKind::Target,
                            key: target.target_key.clone(),
                            application_id: Some(owner.id),
                        },
                        zone_id: zone.id.clone(),
                        drop_class: targets_class(owner.id),
                        draggable: owner.editable,
                    },
                    name: target.name.clone(),
                })
                .collect();
            BoardEnvironment {
                zone,
                label: group.label.clone(),
                targets,
            }
        })
        .collect()
}

fn application(facts: &Facts, state: &ApplicationState) -> BoardApplication {
    let owner = Owner {
        id: state.row.id,
        name: &state.row.name,
        editable: state.capabilities.edit,
    };
    let environments = environments(state, &owner);
    let new_environment = (owner.editable && !environments.is_empty()).then(|| {
        zone(
            BoardZoneKind::NewEnvironment,
            Some(&owner),
            None,
            copy::zone_label(owner.name, &copy::new_environment()),
            Some(copy::new_environment_hint()),
        )
    });
    BoardApplication {
        id: owner.id,
        name: owner.name.to_string(),
        notice: (!owner.editable).then(copy::config_notice),
        build: lane(facts, state, &owner, Stage::Build),
        deploy: lane(facts, state, &owner, Stage::Deploy),
        environments,
        new_environment,
    }
}

fn card_matches(card: &BoardPipeline, needle: &str) -> bool {
    card.name.to_lowercase().contains(needle) || card.repository.to_lowercase().contains(needle)
}

fn application_matches(application: &BoardApplication, needle: &str) -> bool {
    application.name.to_lowercase().contains(needle)
        || application
            .build
            .pipelines
            .iter()
            .chain(&application.deploy.pipelines)
            .any(|card| card_matches(card, needle))
}

fn unassigned(facts: &Facts, needle: Option<&str>) -> BoardLane {
    let assigned: HashSet<&str> = facts
        .states
        .iter()
        .flat_map(|state| &state.members)
        .map(|member| member.row.pipeline_id.as_str())
        .collect();
    let pipelines = sorted(
        facts
            .pipelines
            .values()
            .filter(|pipeline| !assigned.contains(pipeline.id.as_str()))
            .map(|pipeline| pipeline_card(pipeline, None, UNASSIGNED_ZONE))
            .filter(|card| needle.is_none_or(|n| card_matches(card, n)))
            .collect(),
    );
    BoardLane {
        zone: zone(
            BoardZoneKind::Unassigned,
            None,
            None,
            copy::unassigned(),
            Some(copy::unassigned_hint(pipelines.is_empty())),
        ),
        title: copy::unassigned(),
        pipelines,
    }
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
        let needle = needle.as_deref();
        let mut applications: Vec<BoardApplication> = facts
            .states
            .iter()
            .map(|state| application(&facts, state))
            .filter(|app| needle.is_none_or(|n| application_matches(app, n)))
            .collect();
        applications.sort_by_key(|app| (app.name.to_lowercase(), app.id));
        Ok(ApplicationBoard {
            unassigned: unassigned(&facts, needle),
            new_application: zone(
                BoardZoneKind::NewApplication,
                None,
                None,
                copy::new_application(),
                Some(copy::new_application_hint()),
            ),
            applications,
            copy: copy::board_copy(),
        })
    }
}
