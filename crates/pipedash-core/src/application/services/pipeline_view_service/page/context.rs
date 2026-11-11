use std::collections::{
    HashMap,
    HashSet,
};

use pipedash_plugin_api::{
    PluginMetadata,
    PluginRegistry,
};

use super::filters::Filters;
use super::membership::{
    eligible,
    members,
    Item,
};
use super::sort::{
    compare,
    provider_label,
    SortValues,
};
use super::Snapshot;
use crate::domain::{
    MembershipSource,
    ProviderSummary,
    ViewDisplay,
    ViewKind,
    ViewMembership,
    ViewPipeline,
    ViewRef,
    ViewSection,
};

pub(super) struct Context<'a> {
    pub(super) items: Vec<Item<'a>>,
    pub(super) providers: HashMap<i64, &'a ProviderSummary>,
    registry: &'a PluginRegistry,
    pinned: HashSet<&'a str>,
    custom: Vec<(&'a ViewRef, HashMap<usize, MembershipSource>)>,
}

impl Context<'_> {
    pub(super) fn metadata(&self, provider_ids: impl Iterator<Item = i64>) -> Vec<&PluginMetadata> {
        provider_ids
            .filter_map(|id| self.providers.get(&id))
            .filter_map(|provider| self.registry.get(&provider.provider_type))
            .map(|plugin| plugin.metadata())
            .collect()
    }
}

impl<'a> Context<'a> {
    pub(super) fn new(snapshot: &'a Snapshot) -> Self {
        let pinned = snapshot
            .views
            .iter()
            .find(|g| g.view.kind == ViewKind::Pinned)
            .map(|g| g.manual.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let items = eligible(snapshot);
        let custom = snapshot
            .views
            .iter()
            .filter(|g| g.view.kind == ViewKind::Custom)
            .map(|g| {
                let sources = members(&snapshot.views, g, &items, true)
                    .into_iter()
                    .map(|m| (m.index, m.source))
                    .collect();
                (&g.view.id, sources)
            })
            .collect();
        Self {
            items,
            providers: snapshot.providers.iter().map(|p| (p.id, p)).collect(),
            registry: &snapshot.registry,
            pinned,
            custom,
        }
    }

    pub(super) fn ordered_pins(&self, s: &Snapshot, filters: &Filters) -> Vec<usize> {
        let Some(pinned_view) = s.views.iter().find(|g| g.view.kind == ViewKind::Pinned) else {
            return Vec::new();
        };
        let order: HashMap<&str, usize> = pinned_view
            .manual
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let mut indexes: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                order.contains_key(item.pipeline.id.as_str()) && filters.matches(item)
            })
            .map(|(i, _)| i)
            .collect();
        self.order_flat(&mut indexes, &pinned_view.view.display, &order);
        indexes
    }

    pub(super) fn provider_name(&self, id: i64) -> &str {
        self.providers
            .get(&id)
            .map(|p| p.name.as_str())
            .unwrap_or("Unknown")
    }

    pub(super) fn view_pipeline(&self, index: usize) -> ViewPipeline {
        let item = &self.items[index];
        let id = item.pipeline.id.as_str();
        let memberships = self
            .custom
            .iter()
            .filter_map(|(view, sources)| {
                Some(ViewMembership {
                    view: (*view).clone(),
                    source: *sources.get(&index)?,
                })
            })
            .collect();
        ViewPipeline {
            pipeline: item.pipeline.clone(),
            organization: item.organization.clone(),
            pinned: self.pinned.contains(id),
            hidden: item.hidden,
            memberships,
        }
    }

    fn item_values(&self, index: usize, manual: &HashMap<&str, usize>) -> SortValues {
        let item = &self.items[index];
        SortValues {
            name: item.pipeline.name.clone(),
            last_run: item.pipeline.last_run,
            status: Some(item.pipeline.status.clone()),
            organization: item.organization.clone(),
            provider: item.provider.name.clone(),
            manual: manual.get(item.pipeline.id.as_str()).copied(),
        }
    }

    pub(super) fn section_values(
        &self, section: &ViewSection, manual: &HashMap<&str, usize>,
    ) -> SortValues {
        SortValues {
            name: section.title.clone(),
            last_run: section.last_run,
            status: section.status.clone(),
            organization: section
                .organization
                .clone()
                .unwrap_or_else(|| section.title.clone()),
            provider: provider_label(
                section
                    .provider_ids
                    .iter()
                    .map(|id| self.provider_name(*id)),
            ),
            manual: section
                .pipelines
                .first()
                .and_then(|p| manual.get(p.pipeline.id.as_str()).copied()),
        }
    }

    pub(super) fn order_flat(
        &self, indexes: &mut [usize], display: &ViewDisplay, manual: &HashMap<&str, usize>,
    ) {
        indexes.sort_by(|a, b| {
            compare(
                &self.item_values(*a, manual),
                &self.item_values(*b, manual),
                display,
            )
        });
    }
}
