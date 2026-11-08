use std::collections::{
    HashMap,
    HashSet,
};

use super::hierarchy::{
    checked_name,
    denied,
};
use super::snapshot::view_ref;
use super::PipelineViewService;
use crate::domain::{
    CreateViewInput,
    DomainError,
    DomainResult,
    PipelineView,
    ViewCapabilities,
    ViewKind,
    ViewPatch,
    ViewRef,
};

impl PipelineViewService {
    pub async fn create_view(&self, input: CreateViewInput) -> DomainResult<PipelineView> {
        let _guard = self.write_lock.lock().await;
        let stored = self.views.list_views().await?;
        let name = checked_name(&input.name, "View")?;
        Self::unique_name(&stored, &name, None)?;
        let rules = input.rules.validate(&self.known_provider_ids().await?)?;
        let members = self.resolve_pipelines(&input.members).await?;
        let parent = match &input.parent {
            Some(parent) => Some(Self::nest_target(&stored, parent, None)?),
            None => None,
        };

        let id = self
            .views
            .insert_custom(&name, parent.map(|p| p.id), &rules, &input.display)
            .await?;
        if !members.is_empty() {
            self.views.replace_manual_members(id, &members, &[]).await?;
        }
        self.changed().await;
        self.view(&ViewRef::Custom(id)).await
    }

    pub async fn update_view(&self, id: ViewRef, patch: ViewPatch) -> DomainResult<PipelineView> {
        let _guard = self.write_lock.lock().await;
        let stored = self.views.list_views().await?;
        let view = Self::stored(&stored, &id)?;
        let caps = ViewCapabilities::for_kind(view.kind);
        let checks = [
            (patch.name.is_some(), caps.rename, "renamed"),
            (patch.rules.is_some(), caps.edit_rules, "edited"),
            (patch.display.is_some(), caps.edit_display, "edited"),
            (patch.members.is_some(), caps.edit_members, "edited"),
            (
                patch.parent.is_some(),
                view.kind == ViewKind::Custom,
                "moved",
            ),
        ];
        if let Some((_, _, verb)) = checks
            .iter()
            .find(|(present, allowed, _)| *present && !*allowed)
        {
            return Err(denied(&view.name, verb));
        }

        let name = match &patch.name {
            Some(name) => {
                let name = checked_name(name, "View")?;
                Self::unique_name(&stored, &name, Some(view.id))?;
                name
            }
            None => view.name.clone(),
        };
        let rules = match &patch.rules {
            Some(rules) => rules.validate(&self.known_provider_ids().await?)?,
            None => view.rules.clone(),
        };
        let display = patch.display.unwrap_or(view.display);
        let target = match &patch.parent {
            Some(Some(parent)) => Some(Self::nest_target(&stored, parent, Some(view.id))?),
            _ => None,
        };
        let parent_id = match &patch.parent {
            Some(_) => target.map(|t| t.id),
            None => view.parent_id,
        };
        let members = match &patch.members {
            Some(ids) => Some(self.edited_members(view.id, ids, &rules).await?),
            None => None,
        };

        if patch.name.is_some()
            || patch.rules.is_some()
            || patch.display.is_some()
            || patch.parent.is_some()
        {
            self.views
                .update_view(view.id, &name, parent_id, &rules, &display)
                .await?;
        }
        if let Some((kept, excluded)) = members {
            self.views
                .replace_manual_members(view.id, &kept, &excluded)
                .await?;
        }
        self.changed().await;
        self.view(&id).await
    }

    pub async fn delete_view(&self, id: ViewRef) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let stored = self.views.list_views().await?;
        let view = Self::stored(&stored, &id)?;
        if !ViewCapabilities::for_kind(view.kind).delete {
            return Err(denied(&view.name, "deleted"));
        }
        if !self.views.delete_custom(view.id).await? {
            return Err(DomainError::NotFound(format!(
                "Pipeline view {id} not found"
            )));
        }
        self.changed().await;
        Ok(())
    }

    pub async fn reorder_views(&self, order: Vec<ViewRef>) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let stored = self.views.list_views().await?;
        let movable: HashMap<ViewRef, i64> = stored
            .iter()
            .filter(|g| ViewCapabilities::for_kind(g.kind).reorder)
            .map(|g| (view_ref(g), g.id))
            .collect();
        let unique: HashSet<&ViewRef> = order.iter().collect();
        if order.len() != movable.len()
            || unique.len() != order.len()
            || !order.iter().all(|r| movable.contains_key(r))
        {
            return Err(DomainError::InvalidConfig(
                "The new order must list every movable view exactly once".to_string(),
            ));
        }
        let positions: Vec<(i64, i64)> = order
            .iter()
            .enumerate()
            .map(|(index, r)| (movable[r], index as i64))
            .collect();
        self.views.set_positions(&positions).await?;
        self.changed().await;
        Ok(())
    }

    pub async fn reset(&self) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        self.views.reset().await?;
        self.changed().await;
        Ok(())
    }
}
