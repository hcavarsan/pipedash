use std::collections::HashSet;
use std::sync::Arc;

use super::hierarchy::preorder;
use super::page::{
    ResolvedView,
    Snapshot,
};
use super::PipelineViewService;
use crate::domain::{
    DomainResult,
    PinKind,
    PinnedItem,
    PipelineView,
    ViewCapabilities,
    ViewKind,
    ViewRef,
    ViewRules,
};
use crate::infrastructure::database::{
    StoredPin,
    StoredView,
};

pub(super) fn view_ref(view: &StoredView) -> ViewRef {
    match view.kind {
        ViewKind::All => ViewRef::All,
        ViewKind::Pinned => ViewRef::Pinned,
        _ => ViewRef::Custom(view.id),
    }
}

pub(super) fn resolve_pins(pins: Vec<StoredPin>, views: &[StoredView]) -> Vec<PinnedItem> {
    pins.into_iter()
        .filter_map(|pin| {
            let name = match pin.kind {
                PinKind::View => views
                    .iter()
                    .find(|v| v.kind == ViewKind::Custom && v.id.to_string() == pin.reference)?
                    .name
                    .clone(),
                PinKind::Repository => pin
                    .reference
                    .split_once('/')
                    .map_or(pin.reference.as_str(), |(_, rest)| rest)
                    .to_string(),
            };
            Some(PinnedItem {
                kind: pin.kind,
                reference: pin.reference,
                name,
            })
        })
        .collect()
}

impl PipelineViewService {
    pub(super) async fn snapshot(&self) -> DomainResult<Snapshot> {
        let pipelines = self.pipeline_service.cached_pipelines(None).await?;
        let providers = self.provider_service.list_providers().await?;
        let stored = self.views.list_views().await?;
        let members = self.views.list_members().await?;
        let pins = resolve_pins(self.views.list_pins().await?, &stored);
        let config = self.config_state.get().await;

        let mut hidden = HashSet::new();
        let mut resolved: Vec<ResolvedView> = Vec::new();
        let mut ordered: Vec<&StoredView> = stored
            .iter()
            .filter(|g| g.kind != ViewKind::Hidden)
            .collect();
        ordered.sort_by_key(|g| (g.kind != ViewKind::All, g.position, g.id));
        let ordered = preorder(&ordered);
        if let Some(hidden_view) = stored.iter().find(|g| g.kind == ViewKind::Hidden) {
            hidden.extend(
                members
                    .iter()
                    .filter(|m| m.view_id == hidden_view.id && !m.excluded)
                    .map(|m| m.pipeline_id.clone()),
            );
        }
        for view in ordered {
            let id = view_ref(view);
            let own = members.iter().filter(|m| m.view_id == view.id);
            resolved.push(ResolvedView {
                view: PipelineView {
                    id,
                    kind: view.kind,
                    name: view.name.clone(),
                    rules: view.rules.clone(),
                    display: view.display,
                    parent: view.parent_id.map(ViewRef::Custom),
                    capabilities: ViewCapabilities::for_kind(view.kind),
                    count: 0,
                },
                manual: own
                    .clone()
                    .filter(|m| !m.excluded)
                    .map(|m| m.pipeline_id.clone())
                    .collect(),
                excluded: own
                    .filter(|m| m.excluded)
                    .map(|m| m.pipeline_id.clone())
                    .collect(),
            });
        }

        if !config.views.is_empty() {
            let configs = self.repository.config_backend().list_providers().await?;
            for (key, file) in &config.views {
                let provider_ids: Vec<i64> = file
                    .providers
                    .iter()
                    .filter_map(|name| configs.iter().find(|c| c.name == *name)?.id)
                    .collect();
                let rules = if !file.providers.is_empty() && provider_ids.is_empty() {
                    ViewRules::default()
                } else {
                    ViewRules {
                        provider_ids,
                        repositories: file
                            .repositories
                            .iter()
                            .map(|r| r.trim().to_string())
                            .filter(|r| !r.is_empty())
                            .collect(),
                        name_contains: file.name_contains.clone(),
                        statuses: file.statuses.clone(),
                    }
                };
                resolved.push(ResolvedView {
                    view: PipelineView {
                        id: ViewRef::Config(key.clone()),
                        kind: ViewKind::Config,
                        name: file.name.clone(),
                        rules,
                        display: file.display,
                        parent: None,
                        capabilities: ViewCapabilities::for_kind(ViewKind::Config),
                        count: 0,
                    },
                    manual: Vec::new(),
                    excluded: HashSet::new(),
                });
            }
        }

        Ok(Snapshot {
            pipelines,
            providers,
            views: resolved,
            hidden,
            pins,
            registry: Arc::clone(self.provider_service.plugin_registry()),
        })
    }
}
