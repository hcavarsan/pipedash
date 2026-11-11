use std::cmp::Ordering;
use std::collections::{
    BTreeMap,
    BTreeSet,
};

use chrono::{
    DateTime,
    Utc,
};

use super::layout::ordered_groups;
use super::live::{
    primary,
    Live,
};
use super::{
    badness,
    chunked,
    short,
    worst,
    Facts,
    View,
};
use crate::application::services::correlation_service::state::ApplicationState;
use crate::application::services::correlation_service::targets::SHORT_LIVED_GROUP;
use crate::application::services::correlation_service::{
    copy,
    CorrelationService,
    SUMMARY_CHIP_LIMIT,
};
use crate::domain::application::dto::{
    ApplicationList,
    ApplicationListQuery,
    ApplicationSort,
    ApplicationSummary,
    BuildSummary,
    ChipArrow,
    ChipKind,
    TargetChip,
};
use crate::domain::application::Stage;
use crate::domain::pipeline::{
    Pipeline,
    PipelineStatus,
};
use crate::domain::pipeline_view::SortDirection;
use crate::domain::DomainResult;

const BEHIND_PARALLELISM: usize = 8;

pub(super) type Behind = BTreeMap<(i64, String), u32>;

pub(super) struct Row {
    pub summary: ApplicationSummary,
    behind: u32,
    badness: u8,
    last_deploy: Option<DateTime<Utc>>,
    providers: BTreeSet<i64>,
    statuses: Vec<PipelineStatus>,
    names: Vec<String>,
}

fn chip(
    key: &str, label: &str, count: usize, kind: ChipKind, pipelines: &[&Pipeline],
    live: Option<&[Live]>, behind: u32,
) -> TargetChip {
    let lead = worst(pipelines);
    TargetChip {
        group_key: key.to_string(),
        label: label.to_string(),
        commit_short: live.and_then(primary).map(|l| short(&l.sha)),
        status: lead.map_or(PipelineStatus::Pending, |p| p.status.clone()),
        health_label: lead
            .and_then(|p| p.resource_state.as_ref())
            .map(|state| state.label.clone()),
        behind_label: (behind > 0).then(|| copy::behind(behind)),
        count,
        kind,
    }
}

fn summary_chip(chips: &[TargetChip]) -> TargetChip {
    let healthy = chips
        .iter()
        .filter(|c| c.status == PipelineStatus::Success)
        .count();
    let degraded = chips
        .iter()
        .filter(|c| c.status == PipelineStatus::Degraded)
        .count();
    let status = chips
        .iter()
        .map(|c| &c.status)
        .max_by_key(|status| badness(status))
        .cloned()
        .unwrap_or(PipelineStatus::Pending);
    TargetChip {
        group_key: "summary".to_string(),
        label: copy::health_summary(chips.len(), healthy, degraded),
        commit_short: None,
        status,
        health_label: None,
        behind_label: None,
        count: chips.len(),
        kind: ChipKind::Summary,
    }
}

fn chips_of(
    view: &View<'_>, live: &BTreeMap<String, Vec<Live>>, behind: &Behind,
) -> Vec<TargetChip> {
    let id = view.state.row.id;
    let pipelines = view.group_pipelines();
    let mut chips = Vec::new();
    let mut short_lived: Vec<&Pipeline> = Vec::new();
    let mut short_targets = 0;
    for group in ordered_groups(view.state) {
        let found = pipelines
            .get(group.key.as_str())
            .cloned()
            .unwrap_or_default();
        if group.short_lived {
            short_lived.extend(found);
            short_targets += group.targets.len();
            continue;
        }
        let kind = if group.targets.len() == 1 {
            ChipKind::Target
        } else {
            ChipKind::Group
        };
        let behind = behind.get(&(id, group.key.clone())).copied().unwrap_or(0);
        chips.push(chip(
            &group.key,
            &group.label,
            group.targets.len(),
            kind,
            &found,
            live.get(&group.key).map(Vec::as_slice),
            behind,
        ));
    }
    if short_targets > 0 {
        let label = copy::short_lived(short_targets);
        chips.push(chip(
            SHORT_LIVED_GROUP,
            &label,
            short_targets,
            ChipKind::ShortLived,
            &short_lived,
            None,
            0,
        ));
    }
    chips
}

fn last_build<'a>(view: &View<'a>) -> Option<&'a Pipeline> {
    view.members(Stage::Build)
        .filter_map(|m| view.pipeline(&m.row.pipeline_id))
        .max_by(|a, b| (a.last_run, &b.id).cmp(&(b.last_run, &a.id)))
}

pub(super) fn summarize(
    facts: &Facts, state: &ApplicationState, live: &BTreeMap<String, Vec<Live>>, behind: &Behind,
) -> Row {
    let view = View::new(facts, state);
    let id = state.row.id;
    let all = chips_of(&view, live, behind);
    let visible: BTreeSet<&str> = all.iter().map(|c| c.group_key.as_str()).collect();
    let collapsed = all.len() > SUMMARY_CHIP_LIMIT;
    let arrows: Vec<ChipArrow> = if collapsed {
        Vec::new()
    } else {
        state
            .directions
            .iter()
            .filter(|d| visible.contains(d.from_group.as_str()))
            .filter(|d| visible.contains(d.to_group.as_str()))
            .map(|d| ChipArrow {
                from: d.from_group.clone(),
                to: d.to_group.clone(),
            })
            .collect()
    };
    let build = if all.is_empty() {
        last_build(&view).map(|p| BuildSummary {
            label: p.name.clone(),
            status: p.status.clone(),
        })
    } else {
        None
    };
    let mut statuses: Vec<PipelineStatus> = all.iter().map(|c| c.status.clone()).collect();
    statuses.extend(build.iter().map(|b| b.status.clone()));
    let chips = if collapsed {
        vec![summary_chip(&all)]
    } else {
        all
    };
    let members: Vec<&Pipeline> = state
        .members
        .iter()
        .filter_map(|m| view.pipeline(&m.row.pipeline_id))
        .collect();
    Row {
        behind: behind
            .range((id, String::new())..)
            .take_while(|((app, _), _)| *app == id)
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(0),
        badness: statuses.iter().map(badness).max().unwrap_or(0),
        last_deploy: view
            .members(Stage::Deploy)
            .filter_map(|m| view.pipeline(&m.row.pipeline_id))
            .filter_map(|p| p.last_run)
            .max(),
        providers: members.iter().map(|p| p.provider_id).collect(),
        names: members.iter().map(|p| p.name.to_lowercase()).collect(),
        statuses,
        summary: ApplicationSummary {
            id,
            name: state.row.name.clone(),
            identity_kind: state.kind,
            capabilities: state.capabilities,
            chips,
            arrows,
            build,
            suggestion_count: facts
                .suggestions
                .iter()
                .filter(|s| s.applications.contains(&id))
                .count(),
            hidden: state.row.hidden,
            warnings: state.warnings.clone(),
        },
    }
}

fn keeps(row: &Row, query: &ApplicationListQuery) -> bool {
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);
    row.summary.hidden <= query.include_hidden
        && search.is_none_or(|needle| {
            row.summary.name.to_lowercase().contains(&needle)
                || row.names.iter().any(|name| name.contains(&needle))
        })
        && (query.health.is_empty() || row.statuses.iter().any(|s| query.health.contains(s)))
        && (!query.behind_only || row.behind > 0)
        && (query.provider_ids.is_empty()
            || query
                .provider_ids
                .iter()
                .any(|id| row.providers.contains(id)))
        && (!query.with_suggestions || row.summary.suggestion_count > 0)
}

fn by_name(a: &Row, b: &Row) -> Ordering {
    (a.summary.name.to_lowercase(), a.summary.id)
        .cmp(&(b.summary.name.to_lowercase(), b.summary.id))
}

fn order(a: &Row, b: &Row, query: &ApplicationListQuery) -> Ordering {
    let primary = match query.sort {
        ApplicationSort::Name => Ordering::Equal,
        ApplicationSort::Health => a.badness.cmp(&b.badness),
        ApplicationSort::Behind => a.behind.cmp(&b.behind),
        ApplicationSort::LastDeploy => a.last_deploy.cmp(&b.last_deploy),
    };
    let primary = match query.direction {
        SortDirection::Asc => primary,
        SortDirection::Desc => primary.reverse(),
    };
    let name = match (query.sort, query.direction) {
        (ApplicationSort::Name, SortDirection::Desc) => by_name(a, b).reverse(),
        _ => by_name(a, b),
    };
    primary.then(name)
}

impl CorrelationService {
    pub(super) async fn behind_of(
        &self, facts: &Facts, lives: &[BTreeMap<String, Vec<Live>>],
    ) -> Behind {
        let wanted: Vec<(i64, String)> = facts
            .states
            .iter()
            .zip(lives)
            .flat_map(|(state, live)| {
                state
                    .groups
                    .iter()
                    .filter(|g| !g.short_lived && live.contains_key(&g.key))
                    .map(|g| (state.row.id, g.key.clone()))
            })
            .collect();
        let found = chunked(&wanted, BEHIND_PARALLELISM, |(id, key)| async move {
            let behind = self.behind_commits(*id, key).await;
            ((*id, key.clone()), behind)
        })
        .await;
        found
            .into_iter()
            .filter_map(|(key, behind)| Some((key, behind?)))
            .collect()
    }

    pub async fn list(&self, query: ApplicationListQuery) -> DomainResult<ApplicationList> {
        let facts = self.facts(None).await?;
        let lives: Vec<_> = facts
            .states
            .iter()
            .map(|state| View::new(&facts, state).live())
            .collect();
        let behind = self.behind_of(&facts, &lives).await;
        let mut rows: Vec<Row> = facts
            .states
            .iter()
            .zip(&lives)
            .map(|(state, live)| summarize(&facts, state, live, &behind))
            .collect();
        let total = rows.len();
        rows.retain(|row| keeps(row, &query));
        rows.sort_by(|a, b| order(a, b, &query));
        Ok(ApplicationList {
            applications: rows.into_iter().map(|row| row.summary).collect(),
            total,
            suggestion_count: facts.suggestions.len(),
            config_error: self.config_error().await,
            copy: copy::application_copy(),
        })
    }
}
