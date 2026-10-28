mod page;

use std::collections::{
    HashMap,
    HashSet,
};
use std::sync::Arc;

use chrono::Utc;
use page::{
    ResolvedView,
    Snapshot,
};

use super::pipeline_service::PipelineService;
use super::provider_service::ProviderService;
use crate::domain::{
    CreateViewInput,
    DomainError,
    DomainResult,
    LegacyImportResult,
    LegacyMarks,
    PinKind,
    PinnedItem,
    Pipeline,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    ViewCapabilities,
    ViewKind,
    ViewMember,
    ViewPatch,
    ViewRef,
    ViewRules,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::{
    PipelineViewRepository,
    Repository,
    StoredPin,
    StoredView,
};
use crate::infrastructure::ConfigState;

const CONFIG_READ_ONLY: &str =
    "Views from config.toml can't be changed here. Edit config.toml instead.";

fn view_ref(view: &StoredView) -> ViewRef {
    match view.kind {
        ViewKind::All => ViewRef::All,
        ViewKind::Pinned => ViewRef::Pinned,
        _ => ViewRef::Custom(view.id),
    }
}

fn resolve_pins(pins: Vec<StoredPin>, views: &[StoredView]) -> Vec<PinnedItem> {
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

fn denied(name: &str, verb: &str) -> DomainError {
    DomainError::InvalidConfig(format!("{name} can't be {verb}"))
}

fn checked_name(raw: &str, subject: &str) -> DomainResult<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(DomainError::InvalidConfig(format!(
            "{subject} name can't be empty"
        )));
    }
    if name.chars().count() > 60 {
        return Err(DomainError::InvalidConfig(format!(
            "{subject} name can't be longer than 60 characters"
        )));
    }
    Ok(name.to_string())
}

const MAX_DEPTH: usize = 3;

type Nodes = [(i64, Option<i64>)];

fn view_nodes(stored: &[StoredView]) -> Vec<(i64, Option<i64>)> {
    stored
        .iter()
        .filter(|v| v.kind == ViewKind::Custom)
        .map(|v| (v.id, v.parent_id))
        .collect()
}

fn lineage(nodes: &Nodes, id: i64) -> Vec<i64> {
    let mut chain = vec![id];
    let mut current = id;
    while chain.len() <= nodes.len() {
        let Some(parent) = nodes.iter().find(|n| n.0 == current).and_then(|n| n.1) else {
            break;
        };
        chain.push(parent);
        current = parent;
    }
    chain
}

fn height(nodes: &Nodes, id: i64) -> usize {
    1 + nodes
        .iter()
        .filter(|n| n.1 == Some(id))
        .map(|n| height(nodes, n.0))
        .max()
        .unwrap_or(0)
}

fn subtree(nodes: &Nodes, id: i64) -> Vec<i64> {
    nodes
        .iter()
        .filter(|n| n.1 == Some(id))
        .flat_map(|n| std::iter::once(n.0).chain(subtree(nodes, n.0)))
        .collect()
}

fn check_nesting(
    nodes: &Nodes, moving: Option<i64>, parent: i64, subject: &str,
) -> DomainResult<()> {
    let chain = lineage(nodes, parent);
    if moving.is_some_and(|moving| chain.contains(&moving)) {
        return Err(DomainError::InvalidConfig(format!(
            "{subject} can't be moved inside itself"
        )));
    }
    if chain.len() + moving.map_or(1, |moving| height(nodes, moving)) > MAX_DEPTH {
        return Err(DomainError::InvalidConfig(format!(
            "{subject}s can be nested at most {MAX_DEPTH} levels deep"
        )));
    }
    Ok(())
}

fn preorder<'a>(sorted: &[&'a StoredView]) -> Vec<&'a StoredView> {
    fn walk<'a>(parent: Option<i64>, sorted: &[&'a StoredView], out: &mut Vec<&'a StoredView>) {
        for view in sorted.iter().filter(|v| v.parent_id == parent) {
            out.push(view);
            walk(Some(view.id), sorted, out);
        }
    }
    let mut out = Vec::with_capacity(sorted.len());
    walk(None, sorted, &mut out);
    out
}

pub struct PipelineViewService {
    views: PipelineViewRepository,
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    pipeline_service: Arc<PipelineService>,
    config_state: Arc<ConfigState>,
    event_bus: Arc<dyn EventBus>,
    write_lock: tokio::sync::Mutex<()>,
}

impl PipelineViewService {
    pub fn new(
        repository: Arc<Repository>, provider_service: Arc<ProviderService>,
        pipeline_service: Arc<PipelineService>, config_state: Arc<ConfigState>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            views: PipelineViewRepository::new(repository.cache_pool().clone()),
            repository,
            provider_service,
            pipeline_service,
            config_state,
            event_bus,
            write_lock: tokio::sync::Mutex::new(()),
        }
    }

    async fn snapshot(&self) -> DomainResult<Snapshot> {
        let pipelines = self.pipeline_service.get_cached_pipelines(None).await?;
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
        })
    }

    pub async fn list_views(&self) -> DomainResult<Vec<PipelineView>> {
        Ok(page::list_views(&self.snapshot().await?))
    }

    pub async fn page(&self, query: PipelineViewQuery) -> DomainResult<PipelineViewPage> {
        page::build_page(&self.snapshot().await?, &query, Utc::now())
    }

    pub async fn list_pins(&self) -> DomainResult<Vec<PinnedItem>> {
        Ok(resolve_pins(
            self.views.list_pins().await?,
            &self.views.list_views().await?,
        ))
    }

    pub async fn set_pin(&self, kind: PinKind, reference: &str, pinned: bool) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        if pinned {
            let known = match kind {
                PinKind::View => !resolve_pins(
                    vec![StoredPin {
                        kind,
                        reference: reference.to_string(),
                    }],
                    &self.views.list_views().await?,
                )
                .is_empty(),
                PinKind::Repository => {
                    self.cached()
                        .await?
                        .values()
                        .any(|p| p.repository == reference)
                        || self
                            .provider_service
                            .list_providers()
                            .await?
                            .iter()
                            .any(|p| p.configured_repositories.iter().any(|r| r == reference))
                }
            };
            if !known {
                return Err(DomainError::NotFound(format!(
                    "Nothing to pin for {} {reference}",
                    kind.as_str()
                )));
            }
            self.views.insert_pin(kind, reference).await?;
        } else {
            self.views.delete_pin(kind, reference).await?;
        }
        self.changed().await;
        Ok(())
    }

    async fn view(&self, id: &ViewRef) -> DomainResult<PipelineView> {
        self.list_views()
            .await?
            .into_iter()
            .find(|g| g.id == *id)
            .ok_or_else(|| DomainError::NotFound(format!("Pipeline view {id} not found")))
    }

    fn stored<'a>(stored: &'a [StoredView], id: &ViewRef) -> DomainResult<&'a StoredView> {
        let found = match id {
            ViewRef::All => stored.iter().find(|g| g.kind == ViewKind::All),
            ViewRef::Pinned => stored.iter().find(|g| g.kind == ViewKind::Pinned),
            ViewRef::Custom(n) => stored
                .iter()
                .find(|g| g.id == *n && g.kind == ViewKind::Custom),
            ViewRef::Config(_) => {
                return Err(DomainError::InvalidConfig(CONFIG_READ_ONLY.to_string()))
            }
        };
        found.ok_or_else(|| DomainError::NotFound(format!("Pipeline view {id} not found")))
    }

    async fn known_provider_ids(&self) -> DomainResult<HashSet<i64>> {
        Ok(self
            .repository
            .config_backend()
            .list_providers()
            .await?
            .into_iter()
            .filter_map(|p| p.id)
            .collect())
    }

    async fn cached(&self) -> DomainResult<HashMap<String, Pipeline>> {
        Ok(self
            .pipeline_service
            .get_cached_pipelines(None)
            .await?
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect())
    }

    async fn resolve_pipelines(&self, ids: &[String]) -> DomainResult<Vec<(i64, String)>> {
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

    async fn edited_members(
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

    fn unique_name(stored: &[StoredView], name: &str, except: Option<i64>) -> DomainResult<()> {
        let lower = name.to_lowercase();
        if stored.iter().any(|g| {
            g.kind == ViewKind::Custom && Some(g.id) != except && g.name.to_lowercase() == lower
        }) {
            return Err(DomainError::InvalidConfig(format!(
                "A view named \"{name}\" already exists"
            )));
        }
        Ok(())
    }

    fn nest_target<'a>(
        stored: &'a [StoredView], parent: &ViewRef, moving: Option<i64>,
    ) -> DomainResult<&'a StoredView> {
        let target = Self::stored(stored, parent)?;
        if target.kind != ViewKind::Custom {
            return Err(DomainError::InvalidConfig(format!(
                "{} can't contain sub-views",
                target.name
            )));
        }
        check_nesting(&view_nodes(stored), moving, target.id, "View")?;
        Ok(target)
    }

    async fn changed(&self) {
        self.event_bus.emit(CoreEvent::PipelineViewsChanged).await;
    }

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

    async fn cached_pipeline(&self, pipeline_id: &str) -> DomainResult<Pipeline> {
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
        if !ViewCapabilities::for_kind(view.kind).edit_members {
            return Err(denied(&view.name, "edited"));
        }
        let pipeline = self.cached_pipeline(pipeline_id).await?;
        if member {
            self.views
                .upsert_member(view.id, pipeline.provider_id, pipeline_id, false)
                .await?;
        } else {
            let mut targets = vec![view.id];
            targets.extend(subtree(&view_nodes(&stored), view.id));
            for target in stored.iter().filter(|v| targets.contains(&v.id)) {
                if target.rules.matcher().matches(&pipeline) {
                    self.views
                        .upsert_member(target.id, pipeline.provider_id, pipeline_id, true)
                        .await?;
                } else {
                    self.views.delete_member(target.id, pipeline_id).await?;
                }
            }
        }
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

    pub async fn import_legacy_marks(
        &self, marks: LegacyMarks,
    ) -> DomainResult<LegacyImportResult> {
        let _guard = self.write_lock.lock().await;
        let configs = self.repository.config_backend().list_providers().await?;
        let resolve = |key: &str| -> Option<(i64, String)> {
            let config = configs
                .iter()
                .filter(|c| c.id.is_some() && key.starts_with(&format!("{}:", c.display_name())))
                .max_by_key(|c| c.display_name().len())?;
            let remainder = &key[config.display_name().len() + 1..];
            let (kind, rest) = remainder.split_once("__")?;
            if kind != config.provider_type {
                return None;
            }
            let id = config.id?;
            Some((id, format!("{kind}__{id}__{rest}")))
        };

        let stored = self.views.list_views().await?;
        let system = |kind: ViewKind| {
            stored
                .iter()
                .find(|g| g.kind == kind)
                .map(|g| g.id)
                .ok_or_else(|| DomainError::NotFound(format!("{} view not found", kind.as_str())))
        };
        let pinned = system(ViewKind::Pinned)?;
        let hidden = system(ViewKind::Hidden)?;
        let mut existing: HashSet<String> = self
            .views
            .list_members()
            .await?
            .into_iter()
            .filter(|m| m.view_id == pinned && !m.excluded)
            .map(|m| m.pipeline_id)
            .collect();

        let mut result = LegacyImportResult {
            imported: 0,
            skipped: 0,
        };
        for key in &marks.pinned {
            match resolve(key) {
                Some((provider_id, pipeline_id)) => {
                    if existing.insert(pipeline_id.clone()) {
                        self.views
                            .upsert_member(pinned, provider_id, &pipeline_id, false)
                            .await?;
                        result.imported += 1;
                    }
                }
                None => result.skipped += 1,
            }
        }
        for key in &marks.hidden {
            match resolve(key) {
                Some((provider_id, pipeline_id)) => {
                    self.views
                        .upsert_member(hidden, provider_id, &pipeline_id, false)
                        .await?;
                    result.imported += 1;
                }
                None => result.skipped += 1,
            }
        }
        if result.imported > 0 {
            self.changed().await;
        }
        Ok(result)
    }

    pub async fn reset(&self) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        self.views.reset().await?;
        self.changed().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::application::test_support::{
        pipeline,
        Harness,
    };
    use crate::domain::{
        MembershipSource,
        PipelineStatus,
        ProviderConfig,
        ViewMembership,
    };

    fn service(h: &Harness) -> PipelineViewService {
        PipelineViewService::new(
            Arc::clone(&h.repository),
            Arc::clone(&h.provider_service),
            Arc::clone(&h.pipeline_service),
            Arc::clone(&h.config_state),
            h.bus.clone(),
        )
    }

    async fn seed(h: &Harness) -> (i64, Vec<String>) {
        let id = h.add_provider("ci").await;
        let pipelines = vec![
            pipeline(id, "build", PipelineStatus::Success),
            pipeline(id, "deploy", PipelineStatus::Failed),
            pipeline(id, "lint", PipelineStatus::Success),
        ];
        h.repository
            .update_pipelines_cache(id, &pipelines)
            .await
            .unwrap();
        (id, pipelines.into_iter().map(|p| p.id).collect())
    }

    fn input(name: &str) -> CreateViewInput {
        CreateViewInput {
            name: name.to_string(),
            rules: ViewRules::default(),
            display: Default::default(),
            members: Vec::new(),
            parent: None,
        }
    }

    fn child(name: &str, parent: &ViewRef) -> CreateViewInput {
        CreateViewInput {
            parent: Some(parent.clone()),
            ..input(name)
        }
    }

    fn move_to(parent: Option<&ViewRef>) -> ViewPatch {
        ViewPatch {
            parent: Some(parent.cloned()),
            ..Default::default()
        }
    }

    fn invalid(error: DomainError, message: &str) {
        assert!(
            matches!(&error, DomainError::InvalidConfig(m) if m.contains(message)),
            "{error:?}"
        );
    }

    async fn all_page(views: &PipelineViewService) -> PipelineViewPage {
        views.page(PipelineViewQuery::default()).await.unwrap()
    }

    #[tokio::test]
    async fn pipeline_view_mutations_validate_and_emit() {
        let h = Harness::new().await;
        let (_, ids) = seed(&h).await;
        let views = service(&h);

        let created = views.create_view(input("Release")).await.unwrap();
        assert_eq!(h.bus.count("pipeline-views-changed"), 1);
        assert!(matches!(created.id, ViewRef::Custom(_)));

        let duplicate = views.create_view(input(" release ")).await.unwrap_err();
        assert!(matches!(duplicate, DomainError::InvalidConfig(_)));

        let missing = views
            .set_membership(created.id.clone(), "github__1__missing", true)
            .await
            .unwrap_err();
        assert!(matches!(missing, DomainError::NotFound(_)));

        let rename = views
            .update_view(
                ViewRef::All,
                ViewPatch {
                    name: Some("Everything".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(&rename, DomainError::InvalidConfig(m) if m == "All pipelines can't be renamed")
        );
        assert_eq!(h.bus.count("pipeline-views-changed"), 1);

        views
            .set_membership(created.id.clone(), &ids[0], true)
            .await
            .unwrap();
        let page = views
            .page(PipelineViewQuery {
                view: created.id.clone(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.view.count, 1);
    }

    #[tokio::test]
    async fn pipeline_view_editor_keeps_unavailable_members_and_excludes_removed_rule_matches() {
        let h = Harness::new().await;
        let (provider, ids) = seed(&h).await;
        let views = service(&h);
        let created = views
            .create_view(CreateViewInput {
                rules: ViewRules {
                    name_contains: Some("deploy".into()),
                    ..Default::default()
                },
                members: vec![ids[0].clone(), ids[1].clone()],
                ..input("Release")
            })
            .await
            .unwrap();
        let listed = |page: PipelineViewPage| -> Vec<String> {
            page.sections
                .into_iter()
                .flat_map(|s| s.pipelines)
                .map(|p| p.pipeline.name)
                .collect()
        };
        let page_of = || {
            views.page(PipelineViewQuery {
                view: created.id.clone(),
                ..Default::default()
            })
        };
        let members = |ids: &[&String]| ViewPatch {
            members: Some(ids.iter().map(|id| (*id).clone()).collect()),
            ..Default::default()
        };

        let cached = vec![
            pipeline(provider, "deploy", PipelineStatus::Failed),
            pipeline(provider, "lint", PipelineStatus::Success),
        ];
        h.repository
            .update_pipelines_cache(provider, &cached)
            .await
            .unwrap();

        views
            .update_view(created.id.clone(), members(&[&ids[0]]))
            .await
            .unwrap();
        assert!(listed(page_of().await.unwrap()).is_empty());
        let kept = views.list_members(created.id.clone()).await.unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].pipeline_id, ids[0]);
        assert!(!kept[0].available);

        views
            .update_view(created.id.clone(), members(&[&ids[0], &ids[1]]))
            .await
            .unwrap();
        assert_eq!(listed(page_of().await.unwrap()), vec!["deploy"]);

        let missing = views
            .update_view(
                created.id.clone(),
                members(&[&ids[0], &"github__9__gone".to_string()]),
            )
            .await
            .unwrap_err();
        assert!(matches!(missing, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn pipeline_view_nesting_rejects_deep_trees_cycles_and_bad_parents() {
        let h = Harness::new().await;
        seed(&h).await;
        let views = service(&h);

        let top = views.create_view(input("Top")).await.unwrap();
        let mid = views.create_view(child("Mid", &top.id)).await.unwrap();
        let leaf = views.create_view(child("Leaf", &mid.id)).await.unwrap();
        assert_eq!(leaf.parent, Some(mid.id.clone()));

        let deeper = views
            .create_view(child("Deeper", &leaf.id))
            .await
            .unwrap_err();
        invalid(deeper, "Views can be nested at most 3 levels deep");

        let cycle = views
            .update_view(top.id.clone(), move_to(Some(&leaf.id)))
            .await
            .unwrap_err();
        invalid(cycle, "inside itself");
        let itself = views
            .update_view(top.id.clone(), move_to(Some(&top.id)))
            .await
            .unwrap_err();
        invalid(itself, "inside itself");

        let other = views.create_view(input("Other")).await.unwrap();
        views
            .create_view(child("Other child", &other.id))
            .await
            .unwrap();
        let too_tall = views
            .update_view(other.id.clone(), move_to(Some(&mid.id)))
            .await
            .unwrap_err();
        invalid(too_tall, "Views can be nested at most 3 levels deep");

        let unknown = views
            .create_view(child("Ghost", &ViewRef::Custom(9999)))
            .await
            .unwrap_err();
        assert!(matches!(unknown, DomainError::NotFound(_)));
        let system = views
            .create_view(child("Sys", &ViewRef::All))
            .await
            .unwrap_err();
        invalid(system, "can't contain sub-views");
        let config = views
            .create_view(child("Cfg", &ViewRef::Config("x".into())))
            .await
            .unwrap_err();
        assert!(matches!(config, DomainError::InvalidConfig(_)));
        let moved_system = views
            .update_view(ViewRef::All, move_to(Some(&top.id)))
            .await
            .unwrap_err();
        invalid(moved_system, "All pipelines can't be moved");

        let lifted = views
            .update_view(leaf.id.clone(), move_to(None))
            .await
            .unwrap();
        assert_eq!(lifted.parent, None);
        views
            .update_view(other.id.clone(), move_to(Some(&top.id)))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pipeline_view_parent_aggregates_children_and_removal_cascades() {
        let h = Harness::new().await;
        let (_, ids) = seed(&h).await;
        let views = service(&h);

        let parent = views.create_view(input("Parent")).await.unwrap();
        let other = views.create_view(input("Other")).await.unwrap();
        let sub = views.create_view(child("Sub", &parent.id)).await.unwrap();

        let listed = views.list_views().await.unwrap();
        let custom: Vec<&ViewRef> = listed
            .iter()
            .filter(|v| v.kind == ViewKind::Custom)
            .map(|v| &v.id)
            .collect();
        assert_eq!(custom, vec![&parent.id, &sub.id, &other.id]);
        let unchanged = listed.iter().find(|v| v.id == parent.id).unwrap();
        assert_eq!(unchanged.display, parent.display);

        views
            .set_membership(sub.id.clone(), &ids[0], true)
            .await
            .unwrap();
        views
            .set_membership(parent.id.clone(), &ids[1], true)
            .await
            .unwrap();
        let page_of = |view: &ViewRef| {
            views.page(PipelineViewQuery {
                view: view.clone(),
                ..Default::default()
            })
        };

        let page = page_of(&parent.id).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.view.count, 2);
        let section_ids: Vec<&str> = page.sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            section_ids,
            vec![format!("view:{}", sub.id).as_str(), "repo:acme/app"]
        );
        let built = &page.sections[0].pipelines[0];
        assert_eq!(built.pipeline.id, ids[0]);
        assert_eq!(
            built.memberships,
            vec![
                ViewMembership {
                    view: parent.id.clone(),
                    source: MembershipSource::SubView
                },
                ViewMembership {
                    view: sub.id.clone(),
                    source: MembershipSource::Manual
                },
            ]
        );
        let counts: Vec<usize> = views
            .list_views()
            .await
            .unwrap()
            .iter()
            .filter(|v| v.id == parent.id || v.id == sub.id)
            .map(|v| v.count)
            .collect();
        assert_eq!(counts, vec![2, 1]);
        let own = views.list_members(parent.id.clone()).await.unwrap();
        assert_eq!(own.len(), 1);

        views
            .set_membership(parent.id.clone(), &ids[0], false)
            .await
            .unwrap();
        assert_eq!(page_of(&sub.id).await.unwrap().total, 0);
        assert_eq!(page_of(&parent.id).await.unwrap().total, 1);

        views.delete_view(parent.id.clone()).await.unwrap();
        let listed = views.list_views().await.unwrap();
        let orphan = listed.iter().find(|v| v.id == sub.id).unwrap();
        assert_eq!(orphan.parent, None);
    }

    #[tokio::test]
    async fn pipeline_view_legacy_marks_resolve_by_display_name() {
        let h = Harness::new().await;
        let id = h
            .repository
            .add_provider(&ProviderConfig {
                id: None,
                name: "stub-one".into(),
                provider_type: "github".into(),
                token: "token".into(),
                config: HashMap::from([("display_name".to_string(), "Stub One".to_string())]),
                refresh_interval: 30,
                version: None,
            })
            .await
            .unwrap();
        let views = service(&h);

        let result = views
            .import_legacy_marks(LegacyMarks {
                pinned: vec!["Stub One:github__x".into(), "Nobody:github__y".into()],
                hidden: Vec::new(),
            })
            .await
            .unwrap();

        assert_eq!(
            result,
            LegacyImportResult {
                imported: 1,
                skipped: 1
            }
        );
        let members = views.list_members(ViewRef::Pinned).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].pipeline_id, format!("github__{id}__x"));
        assert!(!members[0].available);
        assert_eq!(h.bus.count("pipeline-views-changed"), 1);
    }

    #[tokio::test]
    async fn pipeline_view_pins_validate_list_unpin_and_follow_their_targets() {
        let h = Harness::new().await;
        seed(&h).await;
        let views = service(&h);
        let created = views.create_view(input("Release")).await.unwrap();
        let ViewRef::Custom(id) = created.id.clone() else {
            panic!("custom view expected");
        };

        let unknown = views
            .set_pin(PinKind::Repository, "acme/missing", true)
            .await
            .unwrap_err();
        assert!(matches!(unknown, DomainError::NotFound(_)));
        let system = views.set_pin(PinKind::View, "all", true).await.unwrap_err();
        assert!(matches!(system, DomainError::NotFound(_)));
        assert!(views.list_pins().await.unwrap().is_empty());

        views
            .set_pin(PinKind::Repository, "acme/app", true)
            .await
            .unwrap();
        views
            .set_pin(PinKind::View, &id.to_string(), true)
            .await
            .unwrap();
        views
            .set_pin(PinKind::Repository, "acme/app", true)
            .await
            .unwrap();
        let pins = views.list_pins().await.unwrap();
        assert_eq!(
            pins.iter()
                .map(|p| (p.kind, p.reference.as_str(), p.name.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (PinKind::Repository, "acme/app", "app"),
                (PinKind::View, id.to_string().as_str(), "Release"),
            ]
        );
        assert_eq!(h.bus.count("pipeline-views-changed"), 4);

        views
            .set_pin(PinKind::Repository, "acme/app", false)
            .await
            .unwrap();
        views
            .set_pin(PinKind::Repository, "acme/app", false)
            .await
            .unwrap();
        assert_eq!(views.list_pins().await.unwrap().len(), 1);

        views.delete_view(created.id).await.unwrap();
        assert!(views.list_pins().await.unwrap().is_empty());
        assert!(views.views.list_pins().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn pipeline_view_pinned_blocks_list_repository_pipelines_after_filters() {
        let h = Harness::new().await;
        let (provider, ids) = seed(&h).await;
        let views = service(&h);
        views
            .set_pin(PinKind::Repository, "acme/app", true)
            .await
            .unwrap();

        let page = all_page(&views).await;
        assert_eq!(page.pinned_blocks.len(), 1);
        let block = &page.pinned_blocks[0];
        assert_eq!(
            (block.kind, block.reference.as_str()),
            (PinKind::Repository, "acme/app")
        );
        assert_eq!(block.name, "app");
        assert_eq!(block.pipelines.len(), 3);

        let searched = views
            .page(PipelineViewQuery {
                search: Some("deploy".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(searched.pinned_blocks[0].pipelines.len(), 1);

        let none = views
            .page(PipelineViewQuery {
                search: Some("nothing-matches".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(none.pinned_blocks.is_empty());

        let filtered = views
            .page(PipelineViewQuery {
                provider_id: Some(provider),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(filtered.pinned_blocks.is_empty());

        views.set_hidden(&ids[0], true).await.unwrap();
        assert_eq!(all_page(&views).await.pinned_blocks[0].pipelines.len(), 2);
        let shown = views
            .page(PipelineViewQuery {
                show_hidden: true,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(shown.pinned_blocks[0].pipelines.len(), 3);

        let pinned = views
            .page(PipelineViewQuery {
                view: ViewRef::Pinned,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(pinned.pinned_blocks.len(), 1);

        let custom = views.create_view(input("Release")).await.unwrap();
        let other = views
            .page(PipelineViewQuery {
                view: custom.id,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(other.pinned_blocks.is_empty());

        views.reset().await.unwrap();
        assert!(views.list_pins().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn pipeline_view_config_views_reload_from_config_file() {
        let h = Harness::new().await;
        let path = h._temp_dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[views.release]\nname = \"Release\"\nstatuses = [\"failed\"]\nsort = \"status\"\n",
        )
        .unwrap();

        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();

        let views = service(&h).list_views().await.unwrap();
        let release = views
            .iter()
            .find(|g| g.id == ViewRef::Config("release".into()))
            .unwrap();
        assert_eq!(release.kind, ViewKind::Config);
        assert_eq!(release.display.sort, crate::domain::SortKey::Status);
        assert!(!release.capabilities.edit_display);
        assert_eq!(h.bus.count("pipeline-views-changed"), 1);
    }
}
