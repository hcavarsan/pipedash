use std::collections::{
    BTreeMap,
    HashSet,
};
use std::sync::Arc;

use chrono::{
    DateTime,
    Utc,
};

use super::applications;
use super::edges::{
    apply,
    EdgeSpec,
};
use super::hubs::Hubs;
use super::membership::builder_index;
use super::tags::{
    has_candidates,
    TagEdges,
};
use super::targets;
use super::vocab::{
    BUILDS,
    PIPELINE,
};
use super::world::{
    Affected,
    World,
};
use super::CorrelationService;
use crate::domain::DomainResult;
use crate::infrastructure::database::CorrelationConn;
use crate::infrastructure::PipedashConfig;

pub(super) struct Resolved {
    pub changed: bool,
    pub joined: Vec<String>,
    pub applications: Vec<i64>,
}

pub(super) async fn finish<T>(tx: CorrelationConn, result: DomainResult<T>) -> DomainResult<T> {
    match result {
        Ok(value) => tx.commit().await.map(|()| value),
        Err(error) => {
            if let Err(rollback) = tx.rollback().await {
                tracing::warn!("Correlation rollback failed: {rollback}");
            }
            Err(error)
        }
    }
}

impl CorrelationService {
    pub(super) async fn resolve(&self, changed: &HashSet<String>) -> DomainResult<bool> {
        let mut read = self.store.conn();
        let world = self.load_world(&mut read).await?;
        let affected = self.affected(&mut read, &world, changed).await?;
        let mut tx = self.store.begin().await?;
        let result = self.resolve_in(&mut tx, &world, &affected, None).await;
        let resolved = finish(tx, result).await?;
        if has_candidates(&world, &affected) {
            self.schedule_tags(changed.clone());
        }
        Ok(self.settle(resolved))
    }

    fn schedule_tags(&self, changed: HashSet<String>) {
        let Some(service) = self.arc() else {
            return;
        };
        self.spawn(async move {
            if let Err(error) = service.resolve_tags(&changed).await {
                tracing::warn!(%error, "Correlation could not resolve image tags");
            }
        });
    }

    async fn resolve_tags(&self, changed: &HashSet<String>) -> DomainResult<()> {
        let tags = {
            let mut read = self.store.conn();
            let world = self.load_world(&mut read).await?;
            let affected = self.affected(&mut read, &world, changed).await?;
            self.tag_edges(&world, &affected).await
        };
        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = async {
            let world = self.load_world(&mut tx).await?;
            let affected = self.affected(&mut tx, &world, changed).await?;
            self.resolve_in(&mut tx, &world, &affected, Some(&tags))
                .await
        }
        .await;
        let resolved = finish(tx, result).await?;
        self.settle(resolved);
        Ok(())
    }

    pub async fn resolve_all(&self) -> DomainResult<bool> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.store.begin().await?;
        let result = async {
            let world = self.load_world(&mut tx).await?;
            self.resolve_in(&mut tx, &world, &Affected::everything(), None)
                .await
        }
        .await;
        let resolved = finish(tx, result).await?;
        Ok(self.settle(resolved))
    }

    pub(super) async fn resolve_all_with(
        &self, conn: &mut CorrelationConn, config: Arc<PipedashConfig>,
    ) -> DomainResult<Resolved> {
        let mut world = self.load_world(conn).await?;
        world.config = config;
        self.resolve_in(conn, &world, &Affected::everything(), None)
            .await
    }

    pub(super) async fn resolve_subjects_in(
        &self, conn: &mut CorrelationConn, subjects: &HashSet<String>,
    ) -> DomainResult<Resolved> {
        let world = self.load_world(conn).await?;
        let affected = if subjects.is_empty() {
            Affected::everything()
        } else {
            self.affected(conn, &world, subjects).await?
        };
        self.resolve_in(conn, &world, &affected, None).await
    }

    pub(super) fn settle(&self, resolved: Resolved) -> bool {
        if resolved.changed {
            self.changed();
        }
        self.refetch_history(resolved.joined);
        self.schedule_changes(resolved.applications);
        resolved.changed
    }

    pub(super) async fn resolve_in(
        &self, conn: &mut CorrelationConn, world: &World, affected: &Affected,
        tags: Option<&TagEdges>,
    ) -> DomainResult<Resolved> {
        let now = Utc::now();
        let plan = applications::plan(world);
        let hubs = Hubs::of(world);
        let mut changed = self.sync_exact_edges(conn, world, affected, now).await?;
        changed |= self
            .sync_pair_edges(conn, world, &plan, &hubs, affected, now)
            .await?;
        changed |= self
            .sync_artifact_edges(conn, world, &plan, affected, tags, now)
            .await?;
        let builders = builder_index(world);
        let mut known: HashSet<String> = world
            .members
            .values()
            .flatten()
            .map(|m| m.pipeline_id.clone())
            .collect();
        let mut joined = Vec::new();
        let mut applications = Vec::new();
        let mut suggestions = Vec::new();
        for encoded in self.affected_apps(world, &plan, affected) {
            let Some(app) = plan.apps.get(&encoded) else {
                continue;
            };
            let members = self
                .plan_members(conn, world, &plan, &hubs, &builders, app)
                .await?;
            let planned = targets::compute(world, &plan, app);
            let saved = self.persist_application(conn, world, app, now).await?;
            applications.push(saved.id);
            changed |= saved.changed;
            changed |= self
                .replace_if_changed(
                    conn,
                    saved.id,
                    &members.rows,
                    &planned.rows,
                    &planned.directions,
                )
                .await?;
            for row in &members.rows {
                if known.insert(row.pipeline_id.clone()) {
                    joined.push(row.pipeline_id.clone());
                }
            }
            suggestions.extend(members.suggestions);
        }
        changed |= self.archive_missing(conn, world, &plan, now).await?;
        changed |= self
            .sync_membership_suggestions(conn, world, affected, suggestions, now)
            .await?;
        Ok(Resolved {
            changed,
            joined,
            applications,
        })
    }

    async fn sync_membership_suggestions(
        &self, conn: &mut CorrelationConn, world: &World, affected: &Affected,
        desired: Vec<EdgeSpec>, now: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let mut by_builder: BTreeMap<String, Vec<EdgeSpec>> = BTreeMap::new();
        for spec in desired {
            by_builder
                .entry(spec.from_id.clone())
                .or_default()
                .push(spec);
        }
        let mut changed = false;
        for builder in world.builders().filter(|b| affected.has(&b.id)) {
            let existing = conn
                .list_edges_from(PIPELINE, &builder.id)
                .await?
                .into_iter()
                .filter(|row| row.relation == BUILDS)
                .collect();
            let specs = by_builder.remove(&builder.id).unwrap_or_default();
            let applied = apply(conn, existing, &specs, |_| true, now).await?;
            changed |= applied.changed;
        }
        Ok(changed)
    }
}
