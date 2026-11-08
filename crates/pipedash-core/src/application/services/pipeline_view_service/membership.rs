use std::collections::HashSet;

use super::hierarchy::{
    custom_nodes,
    denied,
    lineage,
    subtree,
    view_nodes,
};
use super::page::{
    self,
    Snapshot,
};
use super::PipelineViewService;
use crate::domain::{
    DomainError,
    DomainResult,
    MembershipSource,
    ViewCapabilities,
    ViewKind,
    ViewMembershipChange,
    ViewRef,
    ViewSelectionEntry,
    ViewSelectionQuery,
};
use crate::infrastructure::database::StoredView;

#[cfg(test)]
mod tests;

fn entries(snapshot: &Snapshot, ids: &[String]) -> Vec<ViewSelectionEntry> {
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let views = page::list_views(snapshot);
    let nodes = custom_nodes(&views);
    let sources = page::custom_sources(snapshot, &wanted);
    views
        .into_iter()
        .filter(|v| v.kind == ViewKind::Custom)
        .zip(sources)
        .filter_map(|(view, (_, sources))| {
            let ViewRef::Custom(id) = view.id else {
                return None;
            };
            let direct_count = sources
                .values()
                .filter(|source| **source != MembershipSource::SubView)
                .count();
            Some(ViewSelectionEntry {
                depth: lineage(&nodes, id).len(),
                selected_count: sources.len(),
                direct_count,
                included_by_rule: sources.values().any(|s| *s == MembershipSource::Rule),
                included_by_sub_view: sources.len() > direct_count,
                view,
            })
        })
        .collect()
}

impl PipelineViewService {
    pub async fn selection(
        &self, query: ViewSelectionQuery,
    ) -> DomainResult<Vec<ViewSelectionEntry>> {
        Ok(entries(&self.snapshot().await?, &query.pipeline_ids))
    }

    pub async fn set_memberships(
        &self, view: ViewRef, change: ViewMembershipChange,
    ) -> DomainResult<Vec<ViewSelectionEntry>> {
        {
            let _guard = self.write_lock.lock().await;
            let stored = self.views.list_views().await?;
            let target = Self::stored(&stored, &view)?;
            let ids: Vec<&str> = change.pipeline_ids.iter().map(String::as_str).collect();
            self.apply_membership(&stored, target, &ids, change.member)
                .await?;
            self.changed().await;
        }
        self.selection(ViewSelectionQuery {
            pipeline_ids: change.pipeline_ids,
        })
        .await
    }

    pub(super) async fn apply_membership(
        &self, stored: &[StoredView], view: &StoredView, ids: &[&str], member: bool,
    ) -> DomainResult<()> {
        if !ViewCapabilities::for_kind(view.kind).edit_members {
            return Err(denied(&view.name, "edited"));
        }
        if view.kind == ViewKind::Pinned && !member {
            for id in ids {
                self.views.delete_member(view.id, id).await?;
            }
            return Ok(());
        }
        let cached = self.cached().await?;
        let mut seen = HashSet::new();
        let mut pipelines = Vec::with_capacity(ids.len());
        for id in ids.iter().filter(|id| seen.insert(**id)) {
            pipelines.push(
                cached
                    .get(*id)
                    .ok_or_else(|| DomainError::NotFound(format!("Pipeline {id} not found")))?,
            );
        }
        let mut targets = vec![view.id];
        if !member {
            targets.extend(subtree(&view_nodes(stored), view.id));
        }
        for pipeline in pipelines {
            if member {
                self.views
                    .upsert_member(view.id, pipeline.provider_id, &pipeline.id, false)
                    .await?;
                continue;
            }
            for target in stored.iter().filter(|v| targets.contains(&v.id)) {
                if target.rules.matcher().matches(pipeline) {
                    self.views
                        .upsert_member(target.id, pipeline.provider_id, &pipeline.id, true)
                        .await?;
                } else {
                    self.views.delete_member(target.id, &pipeline.id).await?;
                }
            }
        }
        Ok(())
    }
}
