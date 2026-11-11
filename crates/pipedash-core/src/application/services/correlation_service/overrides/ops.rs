use std::collections::{
    BTreeSet,
    HashSet,
};

use crate::application::services::correlation_service::copy;
use crate::application::services::correlation_service::state::OpenSuggestion;
use crate::application::services::correlation_service::stored::{
    Identity,
    Op,
};
use crate::application::services::correlation_service::targets::cycles;
use crate::application::services::correlation_service::world::World;
use crate::domain::application::dto::SuggestionKind;
use crate::domain::application::overrides::ApplicationEdit;
use crate::domain::application::IdentityKind;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::{
    ApplicationRow,
    CorrelationConn,
};

const MAX_NAME: usize = 60;

pub(in super::super) fn checked_name(raw: &str) -> DomainResult<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(DomainError::InvalidConfig(copy::name_empty()));
    }
    if name.chars().count() > MAX_NAME {
        return Err(DomainError::InvalidConfig(copy::name_too_long(MAX_NAME)));
    }
    Ok(name.to_string())
}

pub(in super::super) fn new_key() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn invalid(message: String) -> DomainError {
    DomainError::InvalidConfig(message)
}

fn known_pipeline(world: &World, pipeline: &str) -> DomainResult<()> {
    if world.pipelines.contains_key(pipeline) {
        return Ok(());
    }
    Err(invalid(copy::pipeline_not_found(pipeline)))
}

fn member_of(world: &World, id: i64, pipeline: &str) -> bool {
    world
        .members
        .get(&id)
        .is_some_and(|rows| rows.iter().any(|row| row.pipeline_id == pipeline))
}

fn require_member(world: &World, id: i64, pipeline: &str) -> DomainResult<()> {
    if member_of(world, id, pipeline) {
        return Ok(());
    }
    Err(invalid(copy::not_a_member(&world.name_of(pipeline))))
}

pub(in super::super) fn encoded_of(world: &World, id: i64) -> Option<String> {
    world
        .applications
        .iter()
        .find(|(_, row)| row.id == id)
        .map(|(encoded, _)| encoded.clone())
}

pub(in super::super) fn accept_op(world: &World, suggestion: &OpenSuggestion) -> DomainResult<Op> {
    let encoded: Vec<String> = suggestion
        .applications
        .iter()
        .filter_map(|id| encoded_of(world, *id))
        .collect();
    let editable = encoded
        .iter()
        .all(|e| Identity::decode(e).is_some_and(|i| i.kind != IdentityKind::Config));
    if !editable {
        return Err(invalid(copy::suggestion_not_actionable()));
    }
    match (
        suggestion.dto.kind,
        encoded.as_slice(),
        suggestion.pipelines.as_slice(),
    ) {
        (SuggestionKind::SameApplication, [into, from, ..], _) if into != from => Ok(Op::Merge {
            into: into.clone(),
            from: from.clone(),
        }),
        (SuggestionKind::Membership, [application, ..], [builder, ..]) => Ok(Op::Link {
            application: application.clone(),
            pipeline_id: builder.clone(),
            stage: None,
        }),
        _ => Err(invalid(copy::suggestion_not_actionable())),
    }
}

fn find<'a>(open: &'a [OpenSuggestion], id: &str) -> DomainResult<&'a OpenSuggestion> {
    open.iter()
        .find(|s| s.dto.id == id)
        .ok_or_else(|| DomainError::NotFound(copy::suggestion_not_found()))
}

fn merge_op(
    world: &World, row: &ApplicationRow, application: String, other: i64,
) -> DomainResult<Op> {
    if other == row.id {
        return Err(invalid(copy::merge_self()));
    }
    let from = encoded_of(world, other)
        .ok_or_else(|| DomainError::NotFound(copy::application_not_found(other)))?;
    if Identity::decode(&from).is_some_and(|i| i.kind == IdentityKind::Config) {
        return Err(invalid(copy::not_editable_config(&from)));
    }
    Ok(Op::Merge {
        into: application,
        from,
    })
}

async fn target_edit(
    conn: &mut CorrelationConn, row: &ApplicationRow, application: String, edit: ApplicationEdit,
) -> DomainResult<Op> {
    let targets = conn.list_application_targets_by_application(row.id).await?;
    let groups: BTreeSet<String> = targets.iter().map(|t| t.group_key.clone()).collect();
    match edit {
        ApplicationEdit::TargetName { group_key, name } => {
            if !groups.contains(&group_key) {
                return Err(invalid(copy::unknown_group(&group_key)));
            }
            let name = checked_name(&name)?;
            Ok(Op::TargetName {
                application,
                group_key,
                name,
            })
        }
        ApplicationEdit::TargetGroup {
            target_keys,
            group_name,
        } => {
            if target_keys.is_empty() {
                return Err(invalid(copy::no_targets_selected()));
            }
            if let Some(key) = target_keys
                .iter()
                .find(|k| !targets.iter().any(|t| &t.target_key == *k))
            {
                return Err(invalid(copy::unknown_target(key)));
            }
            let group_name = group_name.as_deref().map(checked_name).transpose()?;
            Ok(Op::TargetGroup {
                application,
                target_keys,
                group_name,
            })
        }
        ApplicationEdit::Direction {
            from_group,
            to_group,
            present,
        } => {
            for group in [&from_group, &to_group] {
                if !groups.contains(group) {
                    return Err(invalid(copy::unknown_group(group)));
                }
            }
            if present {
                let stored = conn.list_target_directions_by_application(row.id).await?;
                let existing = stored
                    .into_iter()
                    .map(|d| ((d.from_group, d.to_group), "stored"))
                    .collect();
                if cycles(&existing, &from_group, &to_group) {
                    return Err(invalid(copy::direction_cycle()));
                }
            }
            Ok(Op::Direction {
                application,
                from_group,
                to_group,
                present,
            })
        }
        _ => Err(invalid(copy::suggestion_not_actionable())),
    }
}

pub(in super::super) async fn build_op(
    conn: &mut CorrelationConn, world: &World, row: &ApplicationRow, identity: &Identity,
    edit: ApplicationEdit, open: &[OpenSuggestion],
) -> DomainResult<Op> {
    let application = identity.encode();
    Ok(match edit {
        ApplicationEdit::Link { pipeline_id, stage } => {
            known_pipeline(world, &pipeline_id)?;
            Op::Link {
                application,
                pipeline_id,
                stage,
            }
        }
        ApplicationEdit::Unlink { pipeline_id } => {
            require_member(world, row.id, &pipeline_id)?;
            Op::Unlink {
                application,
                pipeline_id,
            }
        }
        ApplicationEdit::RejectSuggestion { suggestion_id } => {
            find(open, &suggestion_id)?;
            Op::Reject { suggestion_id }
        }
        ApplicationEdit::AcceptSuggestion { suggestion_id } => {
            accept_op(world, find(open, &suggestion_id)?)?
        }
        ApplicationEdit::Merge {
            other_application_id,
        } => merge_op(world, row, application, other_application_id)?,
        ApplicationEdit::Split { pipeline_ids, name } => {
            if pipeline_ids.is_empty() {
                return Err(invalid(copy::no_members()));
            }
            for pipeline in &pipeline_ids {
                require_member(world, row.id, pipeline)?;
            }
            Op::Split {
                key: new_key(),
                name: checked_name(&name)?,
                pipeline_ids,
            }
        }
        ApplicationEdit::Rename { name } => Op::Rename {
            application,
            name: checked_name(&name)?,
        },
        ApplicationEdit::Hide { hidden } => Op::Hide {
            application,
            hidden,
        },
        ApplicationEdit::Stage { pipeline_id, stage } => {
            require_member(world, row.id, &pipeline_id)?;
            Op::Stage {
                application,
                pipeline_id,
                stage,
            }
        }
        other => target_edit(conn, row, application, other).await?,
    })
}

pub(in super::super) fn subjects_of(world: &World, ops: &[&Op], ids: &[i64]) -> HashSet<String> {
    let mut applications: BTreeSet<i64> = ids.iter().copied().collect();
    let mut subjects: HashSet<String> = HashSet::new();
    for op in ops {
        subjects.extend(op.pipelines().into_iter().map(str::to_string));
        let named = op.application().into_iter().chain(match op {
            Op::Merge { from, .. } => Some(from.as_str()),
            _ => None,
        });
        for encoded in named {
            applications.extend(world.applications.get(encoded).map(|row| row.id));
        }
    }
    for id in applications {
        subjects.extend(
            world
                .members
                .get(&id)
                .into_iter()
                .flatten()
                .map(|m| m.pipeline_id.clone()),
        );
    }
    subjects
}
