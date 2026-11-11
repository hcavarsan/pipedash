pub(super) mod entries;
mod ops;
#[cfg(test)]
mod tests;

use std::collections::{
    HashMap,
    HashSet,
};

use chrono::Utc;
use entries::{
    override_entries as entries_of,
    ConfigClaims,
};
use ops::{
    accept_op,
    build_op,
    checked_name,
    new_key,
    subjects_of,
};

use super::copy;
use super::resolve::{
    finish,
    Resolved,
};
use super::stored::{
    Identity,
    MemberSpec,
    Op,
    StoredOverride,
};
use super::CorrelationService;
use crate::domain::application::overrides::{
    ApplicationEdit,
    CreateApplicationInput,
};
use crate::domain::application::{
    ApplicationCapabilities,
    IdentityKind,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::{
    ApplicationRow,
    CorrelationConn,
};

fn root_of(merged: &HashMap<String, String>, application: &str) -> String {
    let mut current = application;
    while let Some(next) = merged.get(current) {
        current = next;
    }
    current.to_string()
}

async fn editable(conn: &mut CorrelationConn, id: i64) -> DomainResult<(ApplicationRow, Identity)> {
    let row = conn
        .get_application(id)
        .await?
        .ok_or_else(|| DomainError::NotFound(copy::application_not_found(id)))?;
    let identity = Identity::of_row(&row)
        .ok_or_else(|| DomainError::InternalError(copy::application_not_found(id)))?;
    if !ApplicationCapabilities::for_identity(identity.kind).edit {
        return Err(DomainError::InvalidConfig(copy::not_editable_config(
            &row.name,
        )));
    }
    Ok((row, identity))
}

impl CorrelationService {
    async fn commit_ops(
        &self, conn: &mut CorrelationConn, world: &super::world::World,
        ops: Vec<(Op, Option<i64>)>, author: Option<String>, ids: &[i64],
    ) -> DomainResult<(Vec<i64>, Resolved)> {
        let now = Utc::now();
        let mut inserted = Vec::new();
        for (op, application) in &ops {
            let row = op.to_row(*application, author.clone(), now);
            inserted.push(conn.insert_override(&row).await?);
        }
        let all: Vec<&Op> = ops.iter().map(|(op, _)| op).collect();
        let subjects: HashSet<String> = subjects_of(world, &all, ids);
        let resolved = self.resolve_subjects_in(conn, &subjects).await?;
        for ((op, application), id) in ops.iter().zip(&inserted) {
            let (Some(key), None) = (op.owned_key(), application) else {
                continue;
            };
            let created = conn
                .get_application_by_identity(IdentityKind::User.as_str(), key)
                .await?;
            conn.set_override_application(*id, created.map(|row| row.id))
                .await?;
        }
        Ok((inserted, resolved))
    }

    async fn conclude(&self, resolved: Resolved) {
        self.settle(resolved);
        self.changed();
        self.flush_events().await;
    }

    pub(super) async fn create_internal(
        &self, input: CreateApplicationInput, author: Option<String>,
    ) -> DomainResult<i64> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = self.create_in(&mut tx, input, author).await;
        let (id, resolved) = finish(tx, result).await?;
        self.conclude(resolved).await;
        Ok(id)
    }

    async fn create_in(
        &self, conn: &mut CorrelationConn, input: CreateApplicationInput, author: Option<String>,
    ) -> DomainResult<(i64, Resolved)> {
        let name = checked_name(&input.name)?;
        if input.members.is_empty() {
            return Err(DomainError::InvalidConfig(copy::no_members()));
        }
        let world = self.load_world(conn).await?;
        for member in &input.members {
            if !world.pipelines.contains_key(&member.pipeline_id) {
                return Err(DomainError::InvalidConfig(copy::pipeline_not_found(
                    &member.pipeline_id,
                )));
            }
        }
        let key = new_key();
        let op = Op::Create {
            key: key.clone(),
            name,
            members: input
                .members
                .into_iter()
                .map(|m| MemberSpec {
                    pipeline_id: m.pipeline_id,
                    stage: m.stage,
                })
                .collect(),
        };
        let (_, resolved) = self
            .commit_ops(conn, &world, vec![(op, None)], author, &[])
            .await?;
        let created = conn
            .get_application_by_identity(IdentityKind::User.as_str(), &key)
            .await?
            .ok_or_else(|| DomainError::InternalError(copy::application_not_found(0)))?;
        Ok((created.id, resolved))
    }

    pub(super) async fn edit_internal(
        &self, id: i64, edit: ApplicationEdit, author: Option<String>,
    ) -> DomainResult<i64> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = self.edit_in(&mut tx, id, edit, author).await;
        let (survivor, resolved) = finish(tx, result).await?;
        self.conclude(resolved).await;
        Ok(survivor)
    }

    async fn edit_in(
        &self, conn: &mut CorrelationConn, id: i64, edit: ApplicationEdit, author: Option<String>,
    ) -> DomainResult<(i64, Resolved)> {
        let (row, identity) = editable(conn, id).await?;
        let world = self.load_world(conn).await?;
        let open = self.open_suggestions(conn, &world).await?;
        let op = build_op(conn, &world, &row, &identity, edit, &open).await?;
        let owner = match &op {
            Op::Split { .. } => None,
            Op::Merge { into, .. }
            | Op::Link {
                application: into, ..
            } => world.applications.get(into).map(|r| r.id).or(Some(id)),
            _ => Some(id),
        };
        let split = matches!(op, Op::Split { .. });
        let key = op.owned_key().map(str::to_string);
        let (_, resolved) = self
            .commit_ops(conn, &world, vec![(op, owner)], author, &[id])
            .await?;
        let survivor = match (split, key) {
            (true, Some(key)) => conn
                .get_application_by_identity(IdentityKind::User.as_str(), &key)
                .await?
                .map_or(id, |created| created.id),
            _ => id,
        };
        Ok((survivor, resolved))
    }

    pub(super) async fn revert_internal(
        &self, application_id: i64, override_id: i64,
    ) -> DomainResult<i64> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = self.revert_in(&mut tx, application_id, override_id).await;
        let (resolved, retag) = finish(tx, result).await?;
        self.conclude(resolved).await;
        if !retag.is_empty() {
            self.resolve(&retag).await?;
        }
        Ok(application_id)
    }

    async fn revert_in(
        &self, conn: &mut CorrelationConn, application_id: i64, override_id: i64,
    ) -> DomainResult<(Resolved, HashSet<String>)> {
        let row = conn
            .get_application(application_id)
            .await?
            .ok_or_else(|| DomainError::NotFound(copy::application_not_found(application_id)))?;
        let world = self.load_world(conn).await?;
        let plan = super::applications::plan(&world);
        let config = ConfigClaims::of(&world);
        let applies = entries_of(&world, &plan, &config, &row)
            .iter()
            .any(|entry| entry.id == override_id);
        let stored = world.overrides.iter().find(|o| o.id == override_id);
        let (true, Some(stored)) = (applies, stored) else {
            return Err(DomainError::NotFound(copy::override_not_found(override_id)));
        };
        let subjects = subjects_of(&world, &[&stored.op], &[application_id]);
        let confirmation = matches!(stored.op, Op::Confirm { .. });
        conn.delete_override(override_id).await?;
        let resolved = self.resolve_subjects_in(conn, &subjects).await?;
        Ok((
            resolved,
            if confirmation {
                subjects
            } else {
                HashSet::new()
            },
        ))
    }

    pub async fn delete_application(&self, id: i64) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = self.delete_in(&mut tx, id).await;
        let resolved = finish(tx, result).await?;
        self.conclude(resolved).await;
        Ok(())
    }

    async fn delete_in(&self, conn: &mut CorrelationConn, id: i64) -> DomainResult<Resolved> {
        let row = conn
            .get_application(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(copy::application_not_found(id)))?;
        let identity = Identity::of_row(&row)
            .ok_or_else(|| DomainError::InternalError(copy::application_not_found(id)))?;
        if !ApplicationCapabilities::for_identity(identity.kind).delete {
            return Err(DomainError::InvalidConfig(copy::not_deletable(&row.name)));
        }
        let world = self.load_world(conn).await?;
        let subjects = subjects_of(&world, &[], &[id]);
        let encoded = identity.encode();
        let related: Vec<i64> = world
            .overrides
            .iter()
            .filter(|o: &&StoredOverride| {
                o.application_id == Some(id)
                    || o.op.owned_key() == Some(identity.key.as_str())
                    || o.op.application() == Some(encoded.as_str())
            })
            .map(|o| o.id)
            .collect();
        for override_id in related {
            conn.delete_override(override_id).await?;
        }
        conn.delete_application(id).await?;
        self.resolve_subjects_in(conn, &subjects).await
    }

    pub async fn accept_suggestions(
        &self, ids: Vec<String>, author: Option<String>,
    ) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = self.accept_in(&mut tx, ids, author).await;
        let resolved = finish(tx, result).await?;
        self.conclude(resolved).await;
        Ok(())
    }

    async fn accept_in(
        &self, conn: &mut CorrelationConn, ids: Vec<String>, author: Option<String>,
    ) -> DomainResult<Resolved> {
        let world = self.load_world(conn).await?;
        let open = self.open_suggestions(conn, &world).await?;
        let mut ops = Vec::new();
        let mut merged: HashMap<String, String> = HashMap::new();
        for id in &ids {
            let Some(suggestion) = open.iter().find(|s| &s.dto.id == id) else {
                continue;
            };
            let op = match accept_op(&world, suggestion)? {
                Op::Merge { into, from } => {
                    let (into, from) = (root_of(&merged, &into), root_of(&merged, &from));
                    if into == from {
                        continue;
                    }
                    merged.insert(from.clone(), into.clone());
                    Op::Merge { into, from }
                }
                other => other,
            };
            let owner = suggestion.applications.first().copied();
            ops.push((op, owner));
        }
        let (_, resolved) = self.commit_ops(conn, &world, ops, author, &[]).await?;
        Ok(resolved)
    }
}
