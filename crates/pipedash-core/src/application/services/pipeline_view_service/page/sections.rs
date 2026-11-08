use std::collections::{
    BTreeSet,
    HashMap,
};

use super::context::Context;
use super::filters::Filters;
use super::membership::Item;
use super::presentation::section_presentation;
use super::sort::most_severe;
use super::Snapshot;
use crate::domain::{
    FetchStatus,
    PinKind,
    PinnedBlock,
    PinnedItem,
    PipelineStatus,
    PipelineViewQuery,
    ProviderFreshness,
    SectionKind,
    SectionMode,
    ViewDisplay,
    ViewPipeline,
    ViewRef,
    ViewSection,
};

pub(super) struct Draft {
    pub(super) id: String,
    pub(super) kind: SectionKind,
    pub(super) title: String,
    pub(super) organization: Option<String>,
    pub(super) status_key: Option<PipelineStatus>,
    pub(super) provider_ids: BTreeSet<i64>,
    pub(super) members: Vec<usize>,
}

pub(super) fn section_key(item: &Item<'_>, mode: SectionMode) -> (String, String, Option<String>) {
    match mode {
        SectionMode::Repository => (
            format!("repo:{}", item.pipeline.repository),
            item.repo.clone(),
            Some(item.organization.clone()),
        ),
        SectionMode::Organization => (
            format!("org:{}", item.organization),
            item.organization.clone(),
            Some(item.organization.clone()),
        ),
        SectionMode::Provider => (
            format!("provider:{}", item.provider.id),
            item.provider.name.clone(),
            None,
        ),
        SectionMode::Status => (
            format!("status:{}", item.pipeline.status.as_str()),
            item.pipeline.status.as_str().to_string(),
            None,
        ),
        SectionMode::None => ("all".to_string(), String::new(), None),
    }
}

pub(super) fn build_section(
    ctx: &Context<'_>, draft: Draft, display: &ViewDisplay, manual: &HashMap<&str, usize>,
) -> ViewSection {
    let mut order = draft.members;
    ctx.order_flat(&mut order, display, manual);
    let pipelines: Vec<ViewPipeline> = order
        .into_iter()
        .map(|index| ctx.view_pipeline(index))
        .collect();
    let mut provider_ids = draft.provider_ids;
    provider_ids.extend(pipelines.iter().map(|p| p.pipeline.provider_id));
    let watched: Vec<&ViewPipeline> = pipelines.iter().filter(|p| !p.hidden).collect();
    let status_source: Vec<&ViewPipeline> = if watched.is_empty() {
        pipelines.iter().collect()
    } else {
        watched
    };
    let status = most_severe(status_source.iter().map(|p| &p.pipeline.status));
    let status_pipeline_id = status.as_ref().and_then(|status| {
        status_source
            .iter()
            .find(|p| p.pipeline.status == *status)
            .map(|p| p.pipeline.id.clone())
    });
    let last_run = pipelines.iter().filter_map(|p| p.pipeline.last_run).max();
    let pending_fetch = provider_ids.iter().any(|id| {
        ctx.providers
            .get(id)
            .is_some_and(|p| matches!(p.last_fetch_status, FetchStatus::Never))
    });
    let presentation = section_presentation(&ctx.metadata(provider_ids.iter().copied()));
    let freshness = ProviderFreshness::aggregate(
        provider_ids
            .iter()
            .filter_map(|id| ctx.providers.get(id))
            .map(|p| &p.freshness),
    );
    ViewSection {
        id: draft.id,
        kind: draft.kind,
        title: draft.title,
        organization: draft.organization,
        provider_ids: provider_ids.into_iter().collect(),
        status: draft.status_key.or(status),
        status_pipeline_id,
        last_run,
        pending_fetch,
        pipelines,
        presentation,
        freshness,
    }
}

impl Context<'_> {
    pub(super) fn pinned_blocks(
        &self, snapshot: &Snapshot, query: &PipelineViewQuery, filters: &Filters,
        display: &ViewDisplay, manual: &HashMap<&str, usize>,
    ) -> Vec<PinnedBlock> {
        let supported = (query.view == ViewRef::All && query.provider_id.is_none())
            || query.view == ViewRef::Pinned;
        if !supported {
            return Vec::new();
        }

        snapshot
            .pins
            .iter()
            .filter(|pin| pin.kind == PinKind::Repository)
            .filter_map(|pin| self.pinned_block(pin, query, filters, display, manual))
            .collect()
    }

    fn pinned_block(
        &self, pin: &PinnedItem, query: &PipelineViewQuery, filters: &Filters,
        display: &ViewDisplay, manual: &HashMap<&str, usize>,
    ) -> Option<PinnedBlock> {
        let mut indexes: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.pipeline.repository == pin.reference
                    && (query.show_hidden || !item.hidden)
                    && filters.matches(item)
            })
            .map(|(i, _)| i)
            .collect();
        if indexes.is_empty() {
            return None;
        }
        self.order_flat(&mut indexes, display, manual);

        Some(PinnedBlock {
            kind: pin.kind,
            reference: pin.reference.clone(),
            name: pin.name.clone(),
            pipelines: indexes
                .into_iter()
                .map(|item| self.view_pipeline(item))
                .collect(),
        })
    }
}
