use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};
use std::sync::Arc;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::PluginRegistry;

use self::context::Context;
use self::filters::Filters;
use self::membership::{
    eligible,
    members,
    split_repository,
};
use self::presentation::view_presentation;
use self::sections::{
    build_section,
    section_key,
    Draft,
};
use self::sort::{
    compare,
    natural_cmp,
    severity,
    SortValues,
};
use crate::domain::{
    DomainError,
    DomainResult,
    MembershipSource,
    PinnedItem,
    Pipeline,
    PipelineMenuEntry,
    PipelineMenuSnapshot,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    ProviderSummary,
    SectionKind,
    SectionMode,
    ViewFacets,
    ViewKind,
    ViewRef,
    ViewSection,
};

mod context;
mod filters;
mod membership;
mod presentation;
mod sections;
mod sort;
#[cfg(test)]
mod tests;

pub(super) struct Snapshot {
    pub pipelines: Vec<Pipeline>,
    pub providers: Vec<ProviderSummary>,
    pub views: Vec<ResolvedView>,
    pub hidden: HashSet<String>,
    pub pins: Vec<PinnedItem>,
    pub registry: Arc<PluginRegistry>,
}

pub(super) struct ResolvedView {
    pub view: PipelineView,
    pub manual: Vec<String>,
    pub excluded: HashSet<String>,
}

pub(super) fn list_views(s: &Snapshot) -> Vec<PipelineView> {
    let items = eligible(s);
    s.views
        .iter()
        .map(|resolved| PipelineView {
            count: members(&s.views, resolved, &items, false).len(),
            ..resolved.view.clone()
        })
        .collect()
}

pub(super) fn custom_sources<'a>(
    s: &'a Snapshot, wanted: &HashSet<&str>,
) -> Vec<(&'a ViewRef, HashMap<&'a str, MembershipSource>)> {
    let items = eligible(s);
    s.views
        .iter()
        .filter(|g| g.view.kind == ViewKind::Custom)
        .map(|g| {
            let sources = members(&s.views, g, &items, true)
                .into_iter()
                .filter_map(|m| {
                    let id = items[m.index].pipeline.id.as_str();
                    wanted.contains(id).then_some((id, m.source))
                })
                .collect();
            (&g.view.id, sources)
        })
        .collect()
}

pub(super) fn menu_snapshot(s: &Snapshot) -> PipelineMenuSnapshot {
    let ctx = Context::new(s);
    let filters = Filters::new(&PipelineViewQuery::default(), Utc::now());
    let mut pipelines: Vec<PipelineMenuEntry> = ctx
        .ordered_pins(s, &filters)
        .into_iter()
        .map(|index| {
            let pipeline = ctx.items[index].pipeline;
            PipelineMenuEntry {
                pipeline_id: pipeline.id.clone(),
                pipeline: Some(pipeline.clone()),
                provider_name: ctx
                    .providers
                    .get(&pipeline.provider_id)
                    .map(|p| p.name.clone()),
            }
        })
        .collect();
    let cached: HashSet<&str> = s.pipelines.iter().map(|p| p.id.as_str()).collect();
    if let Some(pinned) = s.views.iter().find(|g| g.view.kind == ViewKind::Pinned) {
        pipelines.extend(
            pinned
                .manual
                .iter()
                .filter(|id| !cached.contains(id.as_str()))
                .map(|id| PipelineMenuEntry {
                    pipeline_id: id.clone(),
                    pipeline: None,
                    provider_name: None,
                }),
        );
    }
    PipelineMenuSnapshot {
        pipelines,
        shortcuts: s.pins.clone(),
    }
}

pub(super) fn build_page(
    s: &Snapshot, q: &PipelineViewQuery, now: DateTime<Utc>,
) -> DomainResult<PipelineViewPage> {
    let resolved = s
        .views
        .iter()
        .find(|g| g.view.id == q.view)
        .ok_or_else(|| DomainError::NotFound(format!("Pipeline view {} not found", q.view)))?;
    let ctx = Context::new(s);
    let view = &resolved.view;
    let is_all = view.kind == ViewKind::All;
    let display = view.display;
    let filters = Filters::new(q, now);
    let manual_index: HashMap<&str, usize> = resolved
        .manual
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let everyone = members(&s.views, resolved, &ctx.items, true);
    let hidden_count = everyone
        .iter()
        .filter(|m| !m.forced && ctx.items[m.index].hidden)
        .count();
    let visible: Vec<usize> = everyone
        .iter()
        .filter(|m| q.show_hidden || m.forced || !ctx.items[m.index].hidden)
        .map(|m| m.index)
        .collect();

    let mut configured: Vec<(String, BTreeSet<i64>)> = Vec::new();
    if is_all {
        for provider in &s.providers {
            for repository in &provider.configured_repositories {
                match configured.iter_mut().find(|(name, _)| name == repository) {
                    Some((_, ids)) => {
                        ids.insert(provider.id);
                    }
                    None => configured.push((repository.clone(), BTreeSet::from([provider.id]))),
                }
            }
        }
    }

    let mut organizations: BTreeSet<String> = BTreeSet::new();
    let mut facet_providers: BTreeSet<i64> = BTreeSet::new();
    for index in &visible {
        let item = &ctx.items[*index];
        organizations.insert(item.organization.clone());
        facet_providers.insert(item.provider.id);
    }
    for (repository, ids) in &configured {
        if let Some(first) = ids.iter().next() {
            organizations.insert(split_repository(repository, ctx.provider_name(*first)).0);
        }
        facet_providers.extend(ids.iter().copied());
    }
    let mut organizations: Vec<String> = organizations
        .into_iter()
        .filter(|o| o != "Unknown")
        .collect();
    organizations.sort_by(|a, b| natural_cmp(a, b));

    let filtered: Vec<usize> = visible
        .iter()
        .copied()
        .filter(|i| filters.matches(&ctx.items[*i]))
        .collect();

    let sub_views: Vec<(&ResolvedView, HashSet<usize>)> = s
        .views
        .iter()
        .filter(|v| v.view.parent.as_ref() == Some(&view.id))
        .map(|child| {
            let covered = members(&s.views, child, &ctx.items, true)
                .into_iter()
                .map(|m| m.index)
                .collect();
            (child, covered)
        })
        .collect();
    let mode = display.sections;

    let mut sub_view_sections: Vec<ViewSection> = Vec::with_capacity(sub_views.len());
    for (child, covered) in &sub_views {
        let indexes: Vec<usize> = filtered
            .iter()
            .copied()
            .filter(|index| covered.contains(index))
            .collect();
        if indexes.is_empty() {
            continue;
        }
        sub_view_sections.push(build_section(
            &ctx,
            Draft {
                id: format!("view:{}", child.view.id),
                kind: SectionKind::View,
                title: child.view.name.clone(),
                organization: None,
                status_key: None,
                provider_ids: BTreeSet::new(),
                members: indexes,
            },
            &display,
            &manual_index,
        ));
    }

    let mut drafts: Vec<Draft> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    for index in filtered
        .iter()
        .copied()
        .filter(|index| !sub_views.iter().any(|(_, covered)| covered.contains(index)))
    {
        let (id, title, organization) = section_key(&ctx.items[index], mode);
        let draft = *by_id.entry(id.clone()).or_insert_with(|| {
            drafts.push(Draft {
                id,
                kind: mode.into(),
                title,
                organization,
                status_key: (mode == SectionMode::Status)
                    .then(|| ctx.items[index].pipeline.status.clone()),
                provider_ids: BTreeSet::new(),
                members: Vec::new(),
            });
            drafts.len() - 1
        });
        drafts[draft].members.push(index);
    }

    if is_all && mode == SectionMode::Repository {
        let known: HashSet<&str> = ctx
            .items
            .iter()
            .map(|item| item.pipeline.repository.as_str())
            .collect();
        for (repository, ids) in &configured {
            let id = format!("repo:{repository}");
            if let Some(draft) = by_id.get(&id) {
                drafts[*draft].provider_ids.extend(ids.iter().copied());
                continue;
            }
            if known.contains(repository.as_str()) {
                continue;
            }
            let first = ids.iter().next().copied().unwrap_or_default();
            let (organization, repo) = split_repository(repository, ctx.provider_name(first));
            let matches = filters.statuses.is_empty()
                && filters.cutoff.is_none()
                && filters
                    .organization
                    .as_ref()
                    .is_none_or(|o| *o == organization)
                && filters.provider_id.is_none_or(|p| ids.contains(&p))
                && filters.search_matches([
                    repo.as_str(),
                    organization.as_str(),
                    repository.as_str(),
                ]);
            if !matches {
                continue;
            }
            by_id.insert(id.clone(), drafts.len());
            drafts.push(Draft {
                id,
                kind: mode.into(),
                title: repo,
                organization: Some(organization),
                status_key: None,
                provider_ids: ids.clone(),
                members: Vec::new(),
            });
        }
    }

    let mut own_sections: Vec<(SortValues, ViewSection)> = drafts
        .into_iter()
        .filter(|draft| mode != SectionMode::None || !draft.members.is_empty())
        .map(|draft| {
            let section = build_section(&ctx, draft, &display, &manual_index);
            (ctx.section_values(&section, &manual_index), section)
        })
        .collect();
    if mode == SectionMode::Status {
        own_sections.sort_by_key(|(_, section)| severity(section.status.as_ref()));
    } else {
        own_sections.sort_by(|(a, _), (b, _)| compare(a, b, &display));
    }
    let sections: Vec<ViewSection> = sub_view_sections
        .into_iter()
        .chain(own_sections.into_iter().map(|(_, section)| section))
        .collect();

    let pinned = if q.view == ViewRef::All && q.provider_id.is_none() {
        ctx.ordered_pins(s, &filters)
            .into_iter()
            .map(|item| ctx.view_pipeline(item))
            .collect()
    } else {
        Vec::new()
    };

    let pinned_blocks = ctx.pinned_blocks(s, q, &filters, &display, &manual_index);

    let presentation = view_presentation(&ctx.metadata(facet_providers.iter().copied()));
    let count = members(&s.views, resolved, &ctx.items, false).len();
    let mut provider_ids: Vec<i64> = facet_providers.into_iter().collect();
    provider_ids.sort_by(|a, b| natural_cmp(ctx.provider_name(*a), ctx.provider_name(*b)));
    let shown = sections.iter().filter(|s| s.kind != SectionKind::View);
    let section_count = if mode == SectionMode::None {
        shown.map(|s| s.pipelines.len()).sum::<usize>()
    } else {
        shown.count()
    };
    Ok(PipelineViewPage {
        view: PipelineView {
            count,
            ..view.clone()
        },
        pinned,
        pinned_blocks,
        total: sections
            .iter()
            .flat_map(|s| &s.pipelines)
            .map(|p| p.pipeline.id.as_str())
            .collect::<HashSet<_>>()
            .len(),
        sections,
        hidden_count,
        filters_active: filters.active(),
        section_count,
        facets: ViewFacets {
            organizations,
            provider_ids,
        },
        presentation,
    })
}
