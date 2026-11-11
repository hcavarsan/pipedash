use std::cmp::Ordering;
use std::collections::BTreeSet;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::schema::{
    ColumnDataType,
    ColumnDefinition,
};
use serde_json::Value;

use crate::domain::{
    PipelineRun,
    RunHistoryQuery,
    SortDirection,
};

pub(super) struct Criteria<'a> {
    query: &'a RunHistoryQuery,
    search: Option<String>,
    cutoff: Option<DateTime<Utc>>,
}

impl<'a> Criteria<'a> {
    pub(super) fn new(query: &'a RunHistoryQuery, now: DateTime<Utc>) -> Self {
        Self {
            query,
            search: query
                .search
                .as_deref()
                .map(|term| term.trim().to_lowercase())
                .filter(|term| !term.is_empty()),
            cutoff: query
                .date_range
                .map(|range| range.cutoff(query.utc_offset_minutes, now)),
        }
    }

    pub(super) fn matches(&self, run: &PipelineRun) -> bool {
        let query = self.query;
        self.cutoff.is_none_or(|cutoff| run.started_at >= cutoff)
            && (query.statuses.is_empty() || query.statuses.contains(&run.status))
            && same(query.branch.as_deref(), run.branch.as_deref())
            && same(query.actor.as_deref(), run.actor.as_deref())
            && self
                .search
                .as_deref()
                .is_none_or(|term| mentions(run, term))
    }
}

fn same(wanted: Option<&str>, actual: Option<&str>) -> bool {
    match wanted.filter(|value| !value.is_empty()) {
        Some(wanted) => actual == Some(wanted),
        None => true,
    }
}

fn mentions(run: &PipelineRun, term: &str) -> bool {
    [
        run.branch.as_deref(),
        run.commit_sha.as_deref(),
        run.commit_message.as_deref(),
        run.actor.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|field| field.to_lowercase().contains(term))
}

pub(super) fn facets(runs: &[PipelineRun]) -> (Vec<String>, Vec<String>) {
    let collect = |read: fn(&PipelineRun) -> Option<&str>| {
        runs.iter()
            .filter_map(read)
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    (
        collect(|run| run.branch.as_deref()),
        collect(|run| run.actor.as_deref()),
    )
}

#[derive(PartialEq, PartialOrd)]
enum SortValue {
    Missing,
    Number(f64),
    Text(String),
}

fn sort_value(run: &PipelineRun, column: &ColumnDefinition) -> SortValue {
    let Ok(json) = serde_json::to_value(run) else {
        return SortValue::Missing;
    };
    let value = column
        .field_path
        .split('.')
        .try_fold(&json, |current, part| current.as_object()?.get(part));
    match value {
        Some(Value::Number(number)) => number
            .as_f64()
            .map_or(SortValue::Missing, SortValue::Number),
        Some(Value::Bool(flag)) => SortValue::Number(f64::from(u8::from(*flag))),
        Some(Value::String(text)) if column.data_type == ColumnDataType::DateTime => {
            DateTime::parse_from_rfc3339(text).map_or(SortValue::Missing, |time| {
                SortValue::Number(time.timestamp_millis() as f64)
            })
        }
        Some(Value::String(text)) => SortValue::Text(text.to_lowercase()),
        _ => SortValue::Missing,
    }
}

pub(super) fn sort_runs(
    runs: Vec<PipelineRun>, column: &ColumnDefinition, direction: SortDirection,
) -> Vec<PipelineRun> {
    let directed = |order: Ordering| match direction {
        SortDirection::Asc => order,
        SortDirection::Desc => order.reverse(),
    };
    let mut keyed: Vec<(SortValue, PipelineRun)> = runs
        .into_iter()
        .map(|run| (sort_value(&run, column), run))
        .collect();
    keyed.sort_by(|(left_key, left), (right_key, right)| {
        directed(
            left_key
                .partial_cmp(right_key)
                .unwrap_or(Ordering::Equal)
                .then(left.run_number.cmp(&right.run_number)),
        )
    });
    keyed.into_iter().map(|(_, run)| run).collect()
}

#[cfg(test)]
mod tests;
