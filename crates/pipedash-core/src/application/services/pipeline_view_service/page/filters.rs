use chrono::{
    DateTime,
    Utc,
};

use super::membership::Item;
use crate::domain::{
    PipelineStatus,
    PipelineViewQuery,
};

pub(super) struct Filters {
    pub(super) provider_id: Option<i64>,
    pub(super) statuses: Vec<PipelineStatus>,
    pub(super) organization: Option<String>,
    pub(super) cutoff: Option<DateTime<Utc>>,
    search: Option<String>,
}

impl Filters {
    pub(super) fn new(q: &PipelineViewQuery, now: DateTime<Utc>) -> Self {
        Self {
            provider_id: q.provider_id,
            statuses: q.statuses.clone(),
            organization: q.organization.clone().filter(|o| !o.is_empty()),
            cutoff: q
                .date_range
                .map(|range| range.cutoff(q.utc_offset_minutes, now)),
            search: q
                .search
                .as_deref()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty()),
        }
    }

    pub(super) fn active(&self) -> bool {
        self.provider_id.is_some()
            || !self.statuses.is_empty()
            || self.organization.is_some()
            || self.cutoff.is_some()
            || self.search.is_some()
    }

    pub(super) fn search_matches<'a>(&self, values: impl IntoIterator<Item = &'a str>) -> bool {
        match &self.search {
            None => true,
            Some(needle) => values
                .into_iter()
                .any(|value| value.to_lowercase().contains(needle.as_str())),
        }
    }

    pub(super) fn matches(&self, item: &Item<'_>) -> bool {
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
        if !self.statuses.is_empty() && !self.statuses.contains(&p.status) {
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
