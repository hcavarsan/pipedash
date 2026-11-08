use std::cmp::Ordering;

use chrono::{
    DateTime,
    Utc,
};

use crate::domain::{
    PipelineStatus,
    SortDirection,
    SortKey,
    ViewDisplay,
};

const SEVERITY: [PipelineStatus; 7] = [
    PipelineStatus::Failed,
    PipelineStatus::Degraded,
    PipelineStatus::Running,
    PipelineStatus::Pending,
    PipelineStatus::Cancelled,
    PipelineStatus::Success,
    PipelineStatus::Skipped,
];

pub(super) fn severity(status: Option<&PipelineStatus>) -> usize {
    status
        .and_then(|s| SEVERITY.iter().position(|candidate| candidate == s))
        .unwrap_or(SEVERITY.len())
}

pub(super) fn most_severe<'a>(
    statuses: impl Iterator<Item = &'a PipelineStatus>,
) -> Option<PipelineStatus> {
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

pub(super) struct SortValues {
    pub(super) name: String,
    pub(super) last_run: Option<DateTime<Utc>>,
    pub(super) status: Option<PipelineStatus>,
    pub(super) organization: String,
    pub(super) provider: String,
    pub(super) manual: Option<usize>,
}

pub(super) fn compare(a: &SortValues, b: &SortValues, display: &ViewDisplay) -> Ordering {
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

pub(super) fn provider_label<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let mut names: Vec<&str> = names.collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    names.dedup();
    names.join(", ")
}
