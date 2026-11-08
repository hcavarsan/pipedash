use std::collections::{
    HashMap,
    HashSet,
};

use super::PipelineViewService;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    ViewKind,
    ViewMember,
    ViewRef,
    ViewRules,
};

impl PipelineViewService {
    pub(super) async fn known_provider_ids(&self) -> DomainResult<HashSet<i64>> {
        Ok(self
            .repository
            .config_backend()
            .list_providers()
            .await?
            .into_iter()
            .filter_map(|p| p.id)
            .collect())
    }

    pub(super) async fn cached(&self) -> DomainResult<HashMap<String, Pipeline>> {
        Ok(self
            .pipeline_service
            .cached_pipelines(None)
            .await?
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect())
    }

    pub(super) async fn resolve_pipelines(
        &self, ids: &[String],
    ) -> DomainResult<Vec<(i64, String)>> {
        let cached = self.cached().await?;
        let mut seen = HashSet::new();
        let mut resolved = Vec::new();
        for id in ids {
            if !seen.insert(id.as_str()) {
                continue;
            }
            let pipeline = cached
                .get(id)
                .ok_or_else(|| DomainError::NotFound(format!("Pipeline {id} not found")))?;
            resolved.push((pipeline.provider_id, id.clone()));
        }
        Ok(resolved)
    }

    pub(super) async fn edited_members(
        &self, view_id: i64, ids: &[String], rules: &ViewRules,
    ) -> DomainResult<(Vec<(i64, String)>, Vec<(i64, String)>)> {
        let current: HashMap<String, i64> = self
            .views
            .list_members()
            .await?
            .into_iter()
            .filter(|m| m.view_id == view_id && !m.excluded)
            .map(|m| (m.pipeline_id, m.provider_id))
            .collect();
        let cached = self.cached().await?;
        let mut seen = HashSet::new();
        let mut kept = Vec::new();
        for id in ids {
            if !seen.insert(id.as_str()) {
                continue;
            }
            let provider_id = match current.get(id) {
                Some(provider_id) => *provider_id,
                None => {
                    cached
                        .get(id)
                        .ok_or_else(|| DomainError::NotFound(format!("Pipeline {id} not found")))?
                        .provider_id
                }
            };
            kept.push((provider_id, id.clone()));
        }
        let matcher = rules.matcher();
        let mut excluded: Vec<(i64, String)> = current
            .into_iter()
            .filter(|(id, _)| {
                !seen.contains(id.as_str())
                    && !rules.is_empty()
                    && cached.get(id).is_none_or(|p| matcher.matches(p))
            })
            .map(|(id, provider_id)| (provider_id, id))
            .collect();
        excluded.sort();
        Ok((kept, excluded))
    }

    pub async fn list_members(&self, id: ViewRef) -> DomainResult<Vec<ViewMember>> {
        if matches!(id, ViewRef::All | ViewRef::Config(_)) {
            return Ok(Vec::new());
        }
        let stored = self.views.list_views().await?;
        let view = Self::stored(&stored, &id)?;
        let cached = self.cached().await?;
        Ok(self
            .views
            .list_members()
            .await?
            .into_iter()
            .filter(|m| m.view_id == view.id && !m.excluded)
            .map(|m| {
                let pipeline = cached.get(&m.pipeline_id);
                ViewMember {
                    name: pipeline.map(|p| p.name.clone()),
                    repository: pipeline.map(|p| p.repository.clone()),
                    available: pipeline.is_some(),
                    provider_id: m.provider_id,
                    pipeline_id: m.pipeline_id,
                }
            })
            .collect())
    }

    pub(super) async fn cached_pipeline(&self, pipeline_id: &str) -> DomainResult<Pipeline> {
        self.cached()
            .await?
            .remove(pipeline_id)
            .ok_or_else(|| DomainError::NotFound(format!("Pipeline {pipeline_id} not found")))
    }

    pub async fn set_membership(
        &self, id: ViewRef, pipeline_id: &str, member: bool,
    ) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let stored = self.views.list_views().await?;
        let view = Self::stored(&stored, &id)?;
        self.apply_membership(&stored, view, &[pipeline_id], member)
            .await?;
        self.changed().await;
        Ok(())
    }

    pub async fn set_hidden(&self, pipeline_id: &str, hidden: bool) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let stored = self.views.list_views().await?;
        let view = stored
            .iter()
            .find(|g| g.kind == ViewKind::Hidden)
            .ok_or_else(|| DomainError::NotFound("Hidden pipelines view not found".to_string()))?;
        let pipeline = self.cached_pipeline(pipeline_id).await?;
        if hidden {
            self.views
                .upsert_member(view.id, pipeline.provider_id, pipeline_id, false)
                .await?;
        } else {
            self.views.delete_member(view.id, pipeline_id).await?;
        }
        self.changed().await;
        Ok(())
    }
}
