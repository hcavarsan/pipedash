use std::collections::BTreeSet;

use chrono::{
    DateTime,
    Utc,
};

use super::{
    Plan,
    PlannedApp,
};
use crate::application::services::correlation_service::world::{
    Affected,
    World,
};
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ApplicationMemberRow,
    ApplicationRow,
    ApplicationTargetRow,
    CorrelationConn,
    TargetDirectionRow,
};

pub(in super::super) struct Persisted {
    pub id: i64,
    pub changed: bool,
}

impl CorrelationService {
    pub(in super::super) async fn persist_application(
        &self, conn: &mut CorrelationConn, world: &World, app: &PlannedApp, now: DateTime<Utc>,
    ) -> DomainResult<Persisted> {
        let encoded = app.identity.encode();
        let existing = world.applications.get(&encoded);
        let current = existing.is_some_and(|row| {
            row.name == app.name && row.hidden == app.hidden && row.archived_at.is_none()
        });
        if let (Some(row), true) = (existing, current) {
            return Ok(Persisted {
                id: row.id,
                changed: false,
            });
        }
        let row = ApplicationRow {
            id: 0,
            identity_kind: app.identity.kind.as_str().to_string(),
            identity_key: app.identity.key.clone(),
            name: app.name.clone(),
            hidden: app.hidden,
            archived_at: None,
            created_at: existing.map_or(now, |r| r.created_at),
        };
        let id = conn.upsert_application(&row).await?;
        Ok(Persisted { id, changed: true })
    }

    pub(in super::super) async fn archive_missing(
        &self, conn: &mut CorrelationConn, world: &World, plan: &Plan, now: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let mut changed = false;
        for (encoded, row) in &world.applications {
            if plan.apps.contains_key(encoded) || row.archived_at.is_some() {
                continue;
            }
            conn.set_application_archived(row.id, Some(now)).await?;
            changed = true;
        }
        Ok(changed)
    }

    pub(in super::super) async fn replace_if_changed(
        &self, conn: &mut CorrelationConn, id: i64, members: &[ApplicationMemberRow],
        targets: &[ApplicationTargetRow], directions: &[TargetDirectionRow],
    ) -> DomainResult<bool> {
        let mut changed = false;
        let stored = conn.list_application_members_by_application(id).await?;
        let sig = |r: &ApplicationMemberRow| {
            (
                r.pipeline_id.clone(),
                r.stage.clone(),
                r.source.clone(),
                r.edge_id,
            )
        };
        let mut left: Vec<_> = stored.iter().map(sig).collect();
        let mut right: Vec<_> = members.iter().map(sig).collect();
        left.sort();
        right.sort();
        if left != right {
            conn.replace_application_members(id, members).await?;
            changed = true;
        }
        let stored = conn.list_application_targets_by_application(id).await?;
        let key = |r: &ApplicationTargetRow| {
            (
                r.target_key.clone(),
                r.name.clone(),
                r.group_key.clone(),
                r.lifetime.clone(),
            )
        };
        if !stored.iter().map(key).eq(targets.iter().map(key)) {
            conn.replace_application_targets(id, targets).await?;
            changed = true;
        }
        let stored = conn.list_target_directions_by_application(id).await?;
        let key =
            |r: &TargetDirectionRow| (r.from_group.clone(), r.to_group.clone(), r.source.clone());
        if !stored.iter().map(key).eq(directions.iter().map(key)) {
            conn.replace_target_directions(id, directions).await?;
            changed = true;
        }
        Ok(changed)
    }

    pub(in super::super) fn affected_apps(
        &self, world: &World, plan: &Plan, affected: &Affected,
    ) -> BTreeSet<String> {
        if affected.all {
            return plan.apps.keys().cloned().collect();
        }
        let mut found = BTreeSet::new();
        for (encoded, app) in &plan.apps {
            let touched_unit = app
                .units
                .iter()
                .filter_map(|u| world.units.get(u))
                .any(|id| affected.has(id));
            let touched_member = app.explicit.keys().any(|id| affected.has(id));
            if touched_unit || touched_member || !world.applications.contains_key(encoded) {
                found.insert(encoded.clone());
            }
        }
        for (encoded, row) in &world.applications {
            let members = world.members.get(&row.id).into_iter().flatten();
            let vanished = |id: &str| !world.pipelines.contains_key(id);
            if members
                .into_iter()
                .any(|m| affected.has(&m.pipeline_id) || vanished(&m.pipeline_id))
                && plan.apps.contains_key(encoded)
            {
                found.insert(encoded.clone());
            }
        }
        found
    }
}
