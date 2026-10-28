use std::cmp::Ordering;
use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Duration,
    FixedOffset,
    NaiveTime,
    Offset,
    TimeZone,
    Utc,
};

use crate::domain::{
    DateRange,
    DomainError,
    DomainResult,
    FetchStatus,
    MembershipSource,
    PinKind,
    PinnedBlock,
    PinnedItem,
    Pipeline,
    PipelineStatus,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    ProviderSummary,
    SectionKind,
    SectionMode,
    SortDirection,
    SortKey,
    ViewDisplay,
    ViewFacets,
    ViewKind,
    ViewMembership,
    ViewPipeline,
    ViewRef,
    ViewSection,
};

pub(super) struct Snapshot {
    pub pipelines: Vec<Pipeline>,
    pub providers: Vec<ProviderSummary>,
    pub views: Vec<ResolvedView>,
    pub hidden: HashSet<String>,
    pub pins: Vec<PinnedItem>,
}

pub(super) struct ResolvedView {
    pub view: PipelineView,
    pub manual: Vec<String>,
    pub excluded: HashSet<String>,
}

const SEVERITY: [PipelineStatus; 6] = [
    PipelineStatus::Failed,
    PipelineStatus::Running,
    PipelineStatus::Pending,
    PipelineStatus::Cancelled,
    PipelineStatus::Success,
    PipelineStatus::Skipped,
];

fn severity(status: Option<&PipelineStatus>) -> usize {
    status
        .and_then(|s| SEVERITY.iter().position(|candidate| candidate == s))
        .unwrap_or(SEVERITY.len())
}

fn most_severe<'a>(statuses: impl Iterator<Item = &'a PipelineStatus>) -> Option<PipelineStatus> {
    statuses.min_by_key(|s| severity(Some(s))).cloned()
}

pub(super) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => {
                let mut ln = String::new();
                while let Some(c) = left.next_if(char::is_ascii_digit) {
                    ln.push(c);
                }
                let mut rn = String::new();
                while let Some(c) = right.next_if(char::is_ascii_digit) {
                    rn.push(c);
                }
                let ln = ln.trim_start_matches('0');
                let rn = rn.trim_start_matches('0');
                let order = ln.len().cmp(&rn.len()).then_with(|| ln.cmp(rn));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(l), Some(r)) => {
                left.next();
                right.next();
                let order = l.to_lowercase().cmp(r.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

struct Item<'a> {
    pipeline: &'a Pipeline,
    provider: &'a ProviderSummary,
    organization: String,
    repo: String,
    hidden: bool,
}

fn split_repository(repository: &str, provider_name: &str) -> (String, String) {
    match repository.split_once('/') {
        Some((org, rest)) if !org.is_empty() && org != "(root)" && org != "Unknown" => {
            (org.to_string(), rest.to_string())
        }
        Some((_, rest)) => (provider_name.to_string(), rest.to_string()),
        None => (provider_name.to_string(), repository.to_string()),
    }
}

fn eligible(s: &Snapshot) -> Vec<Item<'_>> {
    let providers: HashMap<i64, &ProviderSummary> = s.providers.iter().map(|p| (p.id, p)).collect();
    s.pipelines
        .iter()
        .filter_map(|pipeline| {
            if pipeline.repository.is_empty() {
                tracing::debug!("Skipping pipeline {} without a repository", pipeline.id);
                return None;
            }
            let Some(provider) = providers.get(&pipeline.provider_id).copied() else {
                tracing::debug!(
                    "Skipping pipeline {} of unknown provider {}",
                    pipeline.id,
                    pipeline.provider_id
                );
                return None;
            };
            let (organization, repo) = split_repository(&pipeline.repository, &provider.name);
            Some(Item {
                pipeline,
                provider,
                organization,
                repo,
                hidden: s.hidden.contains(&pipeline.id),
            })
        })
        .collect()
}

struct Member {
    index: usize,
    source: MembershipSource,
    forced: bool,
}

fn descendants<'a>(views: &'a [ResolvedView], id: &ViewRef) -> Vec<&'a ResolvedView> {
    let mut found = Vec::new();
    for child in views.iter().filter(|v| v.view.parent.as_ref() == Some(id)) {
        found.push(child);
        found.extend(descendants(views, &child.view.id));
    }
    found
}

fn members(
    views: &[ResolvedView], view: &ResolvedView, items: &[Item<'_>], show_hidden: bool,
) -> Vec<Member> {
    let mut found: Vec<Option<Member>> = items.iter().map(|_| None).collect();
    let tree = std::iter::once((view, false)).chain(
        descendants(views, &view.view.id)
            .into_iter()
            .map(|child| (child, true)),
    );
    for (resolved, inherited) in tree {
        let manual: HashSet<&str> = resolved.manual.iter().map(String::as_str).collect();
        let matcher = resolved.view.rules.matcher();
        let all = resolved.view.kind == ViewKind::All;
        for (index, item) in items.iter().enumerate() {
            let id = item.pipeline.id.as_str();
            let (source, forced) = if !all && manual.contains(id) {
                (MembershipSource::Manual, true)
            } else if all || (!resolved.excluded.contains(id) && matcher.matches(item.pipeline)) {
                (MembershipSource::Rule, false)
            } else {
                continue;
            };
            match &mut found[index] {
                Some(member) => member.forced |= forced,
                slot => {
                    *slot = Some(Member {
                        index,
                        source: if inherited {
                            MembershipSource::SubView
                        } else {
                            source
                        },
                        forced,
                    })
                }
            }
        }
    }
    found
        .into_iter()
        .flatten()
        .filter(|m| show_hidden || m.forced || !items[m.index].hidden)
        .collect()
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

fn date_cutoff(range: DateRange, utc_offset_minutes: i32, now: DateTime<Utc>) -> DateTime<Utc> {
    match range {
        DateRange::Today => {
            let offset = utc_offset_minutes
                .checked_mul(60)
                .and_then(FixedOffset::east_opt)
                .unwrap_or_else(|| Utc.fix());
            let midnight = now
                .with_timezone(&offset)
                .date_naive()
                .and_time(NaiveTime::MIN);
            offset
                .from_local_datetime(&midnight)
                .single()
                .map(|local| local.with_timezone(&Utc))
                .unwrap_or(now)
        }
        DateRange::Day => now - Duration::hours(24),
        DateRange::Week => now - Duration::days(7),
        DateRange::Month => now - Duration::days(30),
        DateRange::TwoMonths => now - Duration::days(60),
        DateRange::Quarter => now - Duration::days(90),
    }
}

struct Filters {
    provider_id: Option<i64>,
    status: Option<PipelineStatus>,
    organization: Option<String>,
    cutoff: Option<DateTime<Utc>>,
    search: Option<String>,
}

impl Filters {
    fn new(q: &PipelineViewQuery, now: DateTime<Utc>) -> Self {
        Self {
            provider_id: q.provider_id,
            status: q.status.clone(),
            organization: q.organization.clone().filter(|o| !o.is_empty()),
            cutoff: q
                .date_range
                .map(|range| date_cutoff(range, q.utc_offset_minutes, now)),
            search: q
                .search
                .as_deref()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty()),
        }
    }

    fn active(&self) -> bool {
        self.provider_id.is_some()
            || self.status.is_some()
            || self.organization.is_some()
            || self.cutoff.is_some()
            || self.search.is_some()
    }

    fn search_matches<'a>(&self, values: impl IntoIterator<Item = &'a str>) -> bool {
        match &self.search {
            None => true,
            Some(needle) => values
                .into_iter()
                .any(|value| value.to_lowercase().contains(needle.as_str())),
        }
    }

    fn matches(&self, item: &Item<'_>) -> bool {
        let p = item.pipeline;
        if self
            .organization
            .as_ref()
            .is_some_and(|o| *o != item.organization)
        {
            return false;
        }
        if self.provider_id.is_some_and(|id| id != p.provider_id) {
            return false;
        }
        if self.status.as_ref().is_some_and(|s| *s != p.status) {
            return false;
        }
        if let Some(cutoff) = self.cutoff {
            if !p.last_run.is_some_and(|last| last >= cutoff) {
                return false;
            }
        }
        self.search_matches(
            [
                Some(p.name.as_str()),
                p.branch.as_deref(),
                Some(item.repo.as_str()),
                Some(item.organization.as_str()),
                Some(p.repository.as_str()),
            ]
            .into_iter()
            .flatten(),
        )
    }
}

struct SortValues {
    name: String,
    last_run: Option<DateTime<Utc>>,
    status: Option<PipelineStatus>,
    organization: String,
    provider: String,
    manual: Option<usize>,
}

fn compare(a: &SortValues, b: &SortValues, display: &ViewDisplay) -> Ordering {
    let by_name = || natural_cmp(&a.name, &b.name);
    let directed = |order: Ordering| match display.direction {
        SortDirection::Asc => order,
        SortDirection::Desc => order.reverse(),
    };
    match display.sort {
        SortKey::Manual => match (a.manual, b.manual) {
            (Some(x), Some(y)) => x.cmp(&y).then_with(by_name),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => by_name(),
        },
        SortKey::LastRun => match (a.last_run, b.last_run) {
            (Some(x), Some(y)) => directed(x.cmp(&y)).then_with(by_name),
            (None, None) => by_name(),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
        },
        SortKey::Status => directed(severity(a.status.as_ref()).cmp(&severity(b.status.as_ref())))
            .then_with(by_name),
        SortKey::Organization => {
            directed(natural_cmp(&a.organization, &b.organization)).then_with(by_name)
        }
        SortKey::Provider => directed(natural_cmp(&a.provider, &b.provider)).then_with(by_name),
        SortKey::Name => directed(by_name()).then_with(by_name),
    }
}

fn provider_label<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let mut names: Vec<&str> = names.collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    names.dedup();
    names.join(", ")
}

struct Context<'a> {
    items: Vec<Item<'a>>,
    providers: HashMap<i64, &'a ProviderSummary>,
    pinned: HashSet<&'a str>,
    custom: Vec<(&'a ViewRef, HashMap<usize, MembershipSource>)>,
}

impl<'a> Context<'a> {
    fn new(snapshot: &'a Snapshot) -> Self {
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
            pinned,
            custom,
        }
    }

    fn provider_name(&self, id: i64) -> &str {
        self.providers
            .get(&id)
            .map(|p| p.name.as_str())
            .unwrap_or("Unknown")
    }

    fn view_pipeline(&self, index: usize) -> ViewPipeline {
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

    fn section_values(&self, section: &ViewSection, manual: &HashMap<&str, usize>) -> SortValues {
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

    fn order_flat(
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

struct Draft {
    id: String,
    kind: SectionKind,
    title: String,
    organization: Option<String>,
    status_key: Option<PipelineStatus>,
    provider_ids: BTreeSet<i64>,
    members: Vec<usize>,
}

fn section_key(item: &Item<'_>, mode: SectionMode) -> (String, String, Option<String>) {
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

fn build_section(
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
            let matches = filters.status.is_none()
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
        s.views
            .iter()
            .find(|g| g.view.kind == ViewKind::Pinned)
            .map(|pinned_view| {
                let order: HashMap<&str, usize> = pinned_view
                    .manual
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (id.as_str(), i))
                    .collect();
                let mut indexes: Vec<usize> = ctx
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        order.contains_key(item.pipeline.id.as_str()) && filters.matches(item)
                    })
                    .map(|(i, _)| i)
                    .collect();
                ctx.order_flat(&mut indexes, &pinned_view.view.display, &order);
                indexes
                    .into_iter()
                    .map(|item| ctx.view_pipeline(item))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let pinned_blocks =
        if (q.view == ViewRef::All && q.provider_id.is_none()) || q.view == ViewRef::Pinned {
            s.pins
                .iter()
                .filter(|pin| pin.kind == PinKind::Repository)
                .filter_map(|pin| {
                    let mut indexes: Vec<usize> = ctx
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| {
                            item.pipeline.repository == pin.reference
                                && (q.show_hidden || !item.hidden)
                                && filters.matches(item)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    if indexes.is_empty() {
                        return None;
                    }
                    ctx.order_flat(&mut indexes, &display, &manual_index);
                    Some(PinnedBlock {
                        kind: pin.kind,
                        reference: pin.reference.clone(),
                        name: pin.name.clone(),
                        pipelines: indexes
                            .into_iter()
                            .map(|item| ctx.view_pipeline(item))
                            .collect(),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

    let count = members(&s.views, resolved, &ctx.items, false).len();
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
        facets: ViewFacets {
            organizations,
            provider_ids: facet_providers.into_iter().collect(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::test_support::pipeline;
    use crate::domain::{
        ViewCapabilities,
        ViewRules,
    };

    fn provider(id: i64, name: &str) -> ProviderSummary {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": name, "provider_type": "github", "icon": null,
            "pipeline_count": 0, "last_updated": null, "refresh_interval": 30,
            "configured_repositories": [], "last_fetch_status": "success",
            "last_fetch_error": null, "last_fetch_at": null, "version": 1
        }))
        .unwrap()
    }

    fn view(id: ViewRef, kind: ViewKind, display: ViewDisplay) -> ResolvedView {
        ResolvedView {
            view: PipelineView {
                id,
                kind,
                name: kind.as_str().to_string(),
                rules: ViewRules::default(),
                display,
                parent: None,
                capabilities: ViewCapabilities::for_kind(kind),
                count: 0,
            },
            manual: Vec::new(),
            excluded: HashSet::new(),
        }
    }

    fn custom(
        id: i64, name: &str, parent: Option<i64>, manual: &[&str], display: ViewDisplay,
    ) -> ResolvedView {
        let mut resolved = view(ViewRef::Custom(id), ViewKind::Custom, display);
        resolved.view.name = name.to_string();
        resolved.view.parent = parent.map(ViewRef::Custom);
        resolved.manual = manual.iter().map(|n| format!("github__1__{n}")).collect();
        resolved
    }

    fn snapshot(pipelines: Vec<Pipeline>) -> Snapshot {
        Snapshot {
            pipelines,
            providers: vec![provider(1, "Stub One")],
            views: vec![
                view(ViewRef::All, ViewKind::All, ViewDisplay::default()),
                view(
                    ViewRef::Pinned,
                    ViewKind::Pinned,
                    ViewDisplay::pinned_default(),
                ),
            ],
            hidden: HashSet::new(),
            pins: Vec::new(),
        }
    }

    fn query(view: ViewRef) -> PipelineViewQuery {
        PipelineViewQuery {
            view,
            ..Default::default()
        }
    }

    fn named(name: &str, repository: &str, status: PipelineStatus) -> Pipeline {
        Pipeline {
            repository: repository.to_string(),
            ..pipeline(1, name, status)
        }
    }

    fn names(pipelines: &[ViewPipeline]) -> Vec<&str> {
        pipelines.iter().map(|p| p.pipeline.name.as_str()).collect()
    }

    #[test]
    fn hidden_failed_workflow_stays_out_of_repository_status_with_show_hidden() {
        let mut s = snapshot(vec![
            pipeline(1, "build", PipelineStatus::Success),
            pipeline(1, "legacy", PipelineStatus::Failed),
        ]);
        s.hidden.insert("github__1__legacy".into());
        let q = PipelineViewQuery {
            show_hidden: true,
            ..query(ViewRef::All)
        };

        let page = build_page(&s, &q, Utc::now()).unwrap();

        assert_eq!(page.sections.len(), 1);
        let section = &page.sections[0];
        assert_eq!(section.status, Some(PipelineStatus::Success));
        assert_eq!(names(&section.pipelines), vec!["build", "legacy"]);
        assert!(section.pipelines[1].hidden);
        assert_eq!(page.hidden_count, 1);
    }

    #[test]
    fn repository_with_only_hidden_workflows_keeps_their_status() {
        let mut s = snapshot(vec![pipeline(1, "legacy", PipelineStatus::Failed)]);
        s.hidden.insert("github__1__legacy".into());
        let q = PipelineViewQuery {
            show_hidden: true,
            ..query(ViewRef::All)
        };

        let page = build_page(&s, &q, Utc::now()).unwrap();

        assert_eq!(page.sections[0].status, Some(PipelineStatus::Failed));
    }

    #[test]
    fn view_membership_is_manual_plus_rules_minus_excluded() {
        let mut s = snapshot(vec![
            named("a", "other/x", PipelineStatus::Success),
            named("b", "acme/y", PipelineStatus::Success),
            named("c", "acme/z", PipelineStatus::Success),
        ]);
        let mut custom = view(ViewRef::Custom(7), ViewKind::Custom, ViewDisplay::default());
        custom.manual = vec!["github__1__a".into()];
        custom.excluded = HashSet::from(["github__1__c".to_string()]);
        custom.view.rules.repositories = vec!["ACME/*".into()];
        s.views.push(custom);

        let page = build_page(&s, &query(ViewRef::Custom(7)), Utc::now()).unwrap();
        let listed: Vec<&ViewPipeline> = page.sections.iter().flat_map(|s| &s.pipelines).collect();
        let sources: Vec<(&str, MembershipSource)> = listed
            .iter()
            .map(|p| (p.pipeline.name.as_str(), p.memberships[0].source))
            .collect();
        assert_eq!(
            sources,
            vec![
                ("a", MembershipSource::Manual),
                ("b", MembershipSource::Rule)
            ]
        );
        assert_eq!(list_views(&s)[2].count, 2);

        s.hidden.insert("github__1__b".into());
        s.hidden.insert("github__1__a".into());
        let page = build_page(&s, &query(ViewRef::Custom(7)), Utc::now()).unwrap();
        let listed: Vec<&str> = page
            .sections
            .iter()
            .flat_map(|s| names(&s.pipelines))
            .collect();
        assert_eq!(listed, vec!["a"]);
        assert_eq!(page.hidden_count, 1);

        let q = PipelineViewQuery {
            show_hidden: true,
            ..query(ViewRef::Custom(7))
        };
        assert_eq!(build_page(&s, &q, Utc::now()).unwrap().total, 2);
    }

    #[test]
    fn pinned_block_only_on_all_without_provider_filter() {
        let mut s = snapshot(vec![
            pipeline(1, "build", PipelineStatus::Success),
            pipeline(1, "deploy", PipelineStatus::Success),
        ]);
        s.views[1].manual = vec!["github__1__deploy".into(), "github__1__build".into()];

        let page = build_page(&s, &query(ViewRef::All), Utc::now()).unwrap();
        assert_eq!(names(&page.pinned), vec!["deploy", "build"]);
        assert!(page.pinned.iter().all(|p| p.pinned));

        let filtered = PipelineViewQuery {
            provider_id: Some(1),
            ..query(ViewRef::All)
        };
        assert!(build_page(&s, &filtered, Utc::now())
            .unwrap()
            .pinned
            .is_empty());
        let pinned_page = build_page(&s, &query(ViewRef::Pinned), Utc::now()).unwrap();
        assert!(pinned_page.pinned.is_empty());
        assert_eq!(
            names(&pinned_page.sections[0].pipelines),
            vec!["deploy", "build"]
        );
    }

    #[test]
    fn status_sections_follow_severity_order() {
        let mut s = snapshot(vec![
            pipeline(1, "ok", PipelineStatus::Success),
            pipeline(1, "broken", PipelineStatus::Failed),
            pipeline(1, "busy", PipelineStatus::Running),
        ]);
        s.views[0].view.display = ViewDisplay {
            sections: SectionMode::Status,
            direction: SortDirection::Desc,
            ..Default::default()
        };

        let page = build_page(&s, &query(ViewRef::All), Utc::now()).unwrap();
        let ids: Vec<&str> = page.sections.iter().map(|s| s.id.as_str()).collect();

        assert_eq!(
            ids,
            vec!["status:failed", "status:running", "status:success"]
        );
    }

    #[test]
    fn manual_sort_keeps_member_order() {
        let mut s = snapshot(vec![
            named("a", "acme/one", PipelineStatus::Success),
            named("b", "acme/two", PipelineStatus::Success),
            named("c", "acme/three", PipelineStatus::Success),
            named("d", "acme/four", PipelineStatus::Success),
        ]);
        let mut custom = view(
            ViewRef::Custom(3),
            ViewKind::Custom,
            ViewDisplay {
                sort: SortKey::Manual,
                sections: SectionMode::None,
                ..Default::default()
            },
        );
        custom.manual = vec!["github__1__c".into(), "github__1__a".into()];
        custom.view.rules.name_contains = Some("d".into());
        s.views.push(custom);

        let page = build_page(&s, &query(ViewRef::Custom(3)), Utc::now()).unwrap();

        assert_eq!(page.sections.len(), 1);
        assert_eq!(names(&page.sections[0].pipelines), vec!["c", "a", "d"]);
    }

    #[test]
    fn parent_view_lists_sub_view_sections_first_then_its_own_leftovers() {
        let mut s = snapshot(vec![
            named("lib-ci", "acme/lib", PipelineStatus::Success),
            named("app-ci", "acme/app", PipelineStatus::Success),
            named("a1", "acme/x", PipelineStatus::Success),
            named("shared", "acme/x", PipelineStatus::Failed),
            named("b1", "acme/y", PipelineStatus::Success),
        ]);
        s.views.push(custom(
            1,
            "Parent",
            None,
            &["lib-ci", "app-ci", "a1"],
            ViewDisplay::default(),
        ));
        s.views.push(custom(
            2,
            "Alpha",
            Some(1),
            &["a1", "shared"],
            ViewDisplay::default(),
        ));
        s.views.push(custom(
            3,
            "Beta",
            Some(1),
            &["b1", "shared"],
            ViewDisplay::default(),
        ));
        s.views
            .push(custom(4, "Empty", Some(1), &[], ViewDisplay::default()));

        let page = build_page(&s, &query(ViewRef::Custom(1)), Utc::now()).unwrap();
        let sections: Vec<(&str, SectionKind, &str, Vec<&str>)> = page
            .sections
            .iter()
            .map(|s| (s.id.as_str(), s.kind, s.title.as_str(), names(&s.pipelines)))
            .collect();
        assert_eq!(
            sections,
            vec![
                ("view:2", SectionKind::View, "Alpha", vec!["a1", "shared"]),
                ("view:3", SectionKind::View, "Beta", vec!["b1", "shared"]),
                (
                    "repo:acme/app",
                    SectionKind::Repository,
                    "app",
                    vec!["app-ci"]
                ),
                (
                    "repo:acme/lib",
                    SectionKind::Repository,
                    "lib",
                    vec!["lib-ci"]
                ),
            ]
        );
        assert_eq!(page.total, 5);
        assert_eq!(page.view.count, 5);
        let counts: Vec<usize> = list_views(&s).iter().skip(2).map(|v| v.count).collect();
        assert_eq!(counts, vec![5, 2, 2, 0]);

        let own = &page.sections[2].pipelines[0];
        let own_sources: Vec<(ViewRef, MembershipSource)> = own
            .memberships
            .iter()
            .map(|m| (m.view.clone(), m.source))
            .collect();
        assert_eq!(
            own_sources,
            vec![(ViewRef::Custom(1), MembershipSource::Manual)]
        );
        for section in &page.sections[..2] {
            let shared = &section.pipelines[1];
            let sources: Vec<(ViewRef, MembershipSource)> = shared
                .memberships
                .iter()
                .map(|m| (m.view.clone(), m.source))
                .collect();
            assert_eq!(
                sources,
                vec![
                    (ViewRef::Custom(1), MembershipSource::SubView),
                    (ViewRef::Custom(2), MembershipSource::Manual),
                    (ViewRef::Custom(3), MembershipSource::Manual),
                ]
            );
        }

        let filtered = PipelineViewQuery {
            search: Some("b1".into()),
            ..query(ViewRef::Custom(1))
        };
        let page = build_page(&s, &filtered, Utc::now()).unwrap();
        assert_eq!(page.sections.len(), 1);
        assert_eq!(page.sections[0].id, "view:3");
        assert_eq!(page.total, 1);
    }

    #[test]
    fn sub_view_sections_lead_whatever_the_sort_and_own_section_mode() {
        let mut s = snapshot(vec![
            named("own", "acme/x", PipelineStatus::Success),
            named("a1", "acme/x", PipelineStatus::Success),
            named("z1", "acme/x", PipelineStatus::Failed),
        ]);
        let display = ViewDisplay {
            sections: SectionMode::None,
            direction: SortDirection::Desc,
            ..Default::default()
        };
        s.views.push(custom(1, "Parent", None, &["own"], display));
        s.views.push(custom(
            2,
            "Kids",
            Some(1),
            &["a1", "z1"],
            ViewDisplay::default(),
        ));

        let page = build_page(&s, &query(ViewRef::Custom(1)), Utc::now()).unwrap();
        let sections: Vec<(&str, SectionKind, Vec<&str>)> = page
            .sections
            .iter()
            .map(|s| (s.id.as_str(), s.kind, names(&s.pipelines)))
            .collect();
        assert_eq!(
            sections,
            vec![
                ("view:2", SectionKind::View, vec!["z1", "a1"]),
                ("all", SectionKind::None, vec!["own"]),
            ]
        );
        assert_eq!(page.sections[0].status, Some(PipelineStatus::Failed));
        assert_eq!(page.total, 3);
    }

    #[test]
    fn search_matches_full_repository_path() {
        let s = snapshot(vec![
            named("build", "acme/app", PipelineStatus::Success),
            named("deploy", "acme/other", PipelineStatus::Success),
        ]);
        let q = PipelineViewQuery {
            search: Some("acme/app".into()),
            ..query(ViewRef::All)
        };

        let page = build_page(&s, &q, Utc::now()).unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(names(&page.sections[0].pipelines), vec!["build"]);
    }

    #[test]
    fn natural_cmp_orders_numbers_numerically_and_ignores_case() {
        assert_eq!(natural_cmp("build-2", "Build-10"), Ordering::Less);
        assert_eq!(natural_cmp("Alpha", "alpha"), Ordering::Equal);
        assert_eq!(natural_cmp("app", "Beta"), Ordering::Less);
    }

    #[test]
    fn today_cutoff_uses_client_offset() {
        let now = "2026-10-27T02:00:00Z".parse::<DateTime<Utc>>().unwrap();

        assert_eq!(
            date_cutoff(DateRange::Today, -300, now),
            "2026-10-26T05:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(
            date_cutoff(DateRange::Day, 0, now),
            "2026-10-26T02:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
    }

    #[test]
    fn unknown_view_is_not_found() {
        let s = snapshot(Vec::new());

        let error = build_page(&s, &query(ViewRef::Custom(99)), Utc::now()).unwrap_err();

        assert!(matches!(error, DomainError::NotFound(_)));
    }
}
