use serde::{
    Deserialize,
    Serialize,
};

use super::date_range::DateRange;
use super::pipeline::{
    PipelineRun,
    PipelineStatus,
};
use super::pipeline_view::SortDirection;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSort {
    pub column: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedRunSort {
    pub column: Option<String>,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RunHistoryQuery {
    pub page: usize,
    pub page_size: usize,
    pub search: Option<String>,
    pub statuses: Vec<PipelineStatus>,
    pub branch: Option<String>,
    pub actor: Option<String>,
    pub date_range: Option<DateRange>,
    pub utc_offset_minutes: i32,
    pub sort: Option<RunSort>,
    pub scan_depth: usize,
}

impl Default for RunHistoryQuery {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 20,
            search: None,
            statuses: Vec::new(),
            branch: None,
            actor: None,
            date_range: None,
            utc_offset_minutes: 0,
            sort: None,
            scan_depth: 1,
        }
    }
}

impl RunHistoryQuery {
    pub fn page(page: usize, page_size: usize) -> Self {
        Self {
            page,
            page_size,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHistoryScanNotice {
    pub title: String,
    pub message: String,
    pub action_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHistoryPage {
    pub runs: Vec<PipelineRun>,
    pub total_count: usize,
    pub has_more: bool,
    pub can_scan_deeper: bool,
    pub is_complete: bool,
    pub scan_notice: Option<RunHistoryScanNotice>,
    pub sort: AppliedRunSort,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    pub branches: Vec<String>,
    pub actors: Vec<String>,
}
