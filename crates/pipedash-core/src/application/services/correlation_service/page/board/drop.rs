use crate::application::services::correlation_service::copy::board as copy;
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::application::board::{
    BoardCardKind,
    BoardCardRef,
    BoardDrop,
    BoardDropOutcome,
    BoardZoneKind,
    BoardZoneRef,
};
use crate::domain::application::overrides::ApplicationEdit;
use crate::domain::application::Stage;
use crate::domain::{
    DomainError,
    DomainResult,
};

enum Planned {
    Edit(i64, ApplicationEdit),
    Ask(BoardDropOutcome),
}

fn refused() -> DomainError {
    DomainError::InvalidConfig(copy::drop_not_allowed())
}

fn pipeline_drop(card: BoardCardRef, zone: BoardZoneRef) -> DomainResult<Planned> {
    let pipeline_id = card.key;
    match zone.kind {
        BoardZoneKind::NewApplication => Ok(Planned::Ask(BoardDropOutcome::NameApplication {
            pipeline_id,
        })),
        BoardZoneKind::Unassigned => {
            let application = card.application_id.ok_or_else(refused)?;
            Ok(Planned::Edit(
                application,
                ApplicationEdit::Unlink { pipeline_id },
            ))
        }
        BoardZoneKind::Lane => {
            let application = zone.application_id.ok_or_else(refused)?;
            let stage = zone
                .key
                .as_deref()
                .and_then(Stage::parse)
                .ok_or_else(refused)?;
            let edit = if card.application_id == Some(application) {
                ApplicationEdit::Stage { pipeline_id, stage }
            } else {
                ApplicationEdit::Link {
                    pipeline_id,
                    stage: Some(stage),
                }
            };
            Ok(Planned::Edit(application, edit))
        }
        BoardZoneKind::Environment | BoardZoneKind::NewEnvironment => Err(refused()),
    }
}

fn target_drop(card: BoardCardRef, zone: BoardZoneRef) -> DomainResult<Planned> {
    let application_id = card
        .application_id
        .filter(|id| zone.application_id == Some(*id))
        .ok_or_else(refused)?;
    match zone.kind {
        BoardZoneKind::NewEnvironment => Ok(Planned::Ask(BoardDropOutcome::NameEnvironment {
            application_id,
            target_key: card.key,
        })),
        BoardZoneKind::Environment => Ok(Planned::Edit(
            application_id,
            ApplicationEdit::TargetGroup {
                target_keys: vec![card.key],
                group_name: None,
                group_key: Some(zone.key.ok_or_else(refused)?),
            },
        )),
        _ => Err(refused()),
    }
}

impl CorrelationService {
    pub async fn board_drop(
        &self, drop: BoardDrop, author: Option<String>,
    ) -> DomainResult<BoardDropOutcome> {
        let planned = match drop.card.kind {
            BoardCardKind::Pipeline => pipeline_drop(drop.card, drop.zone)?,
            BoardCardKind::Target => target_drop(drop.card, drop.zone)?,
        };
        match planned {
            Planned::Ask(outcome) => Ok(outcome),
            Planned::Edit(application, edit) => {
                self.edit_internal(application, edit, author).await?;
                Ok(BoardDropOutcome::Applied)
            }
        }
    }
}
