use std::collections::BTreeMap;

use pipedash_plugin_api::CommitSha;

use super::layout::ordered_groups;
use super::live::Live;
use super::{
    chunked,
    repo_id,
    short,
    View,
};
use crate::application::services::correlation_service::changes::segments::segments;
use crate::application::services::correlation_service::changes::{
    ChangeData,
    StageRun,
};
use crate::application::services::correlation_service::copy::{
    self,
    views,
};
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::application::dto::{
    CellState,
    ChangeCell,
    ChangeColumn,
    ChangeRow,
    ChangesPage,
};
use crate::domain::pipeline::PipelineStatus;
use crate::domain::{
    DomainError,
    DomainResult,
};

const MAX_LIMIT: usize = 200;
const BUILD_COLUMN: &str = "build";
const MESSAGE_PARALLELISM: usize = 8;

fn latest<'a>(runs: impl Iterator<Item = &'a StageRun>) -> Option<&'a StageRun> {
    runs.max_by_key(|run| (run.event_at(), run.run_number))
}

fn state_of(run: Option<&StageRun>) -> CellState {
    let Some(run) = run.filter(|run| run.outcome_recorded) else {
        return CellState::NoEvidence;
    };
    let status = run.status.as_str();
    if status == PipelineStatus::Success.as_str() {
        CellState::Success
    } else if status == PipelineStatus::Failed.as_str()
        || status == PipelineStatus::Degraded.as_str()
    {
        CellState::Failed
    } else if status == PipelineStatus::Running.as_str()
        || status == PipelineStatus::Pending.as_str()
    {
        CellState::Running
    } else {
        CellState::NoEvidence
    }
}

fn cell(column: &str, run: Option<&StageRun>, live: bool) -> ChangeCell {
    let label = if live {
        Some(copy::location_live())
    } else {
        run.map(|run| views::run_label(run.run_number))
    };
    ChangeCell {
        column: column.to_string(),
        state: if live { CellState::Live } else { state_of(run) },
        pipeline_id: run.map(|run| run.pipeline_id.clone()),
        run_number: run.map(|run| run.run_number),
        label,
    }
}

fn is_live(live: &BTreeMap<String, Vec<Live>>, group: &str, change: &ChangeData) -> bool {
    live.get(group)
        .is_some_and(|found| found.iter().any(|l| l.is(&change.repo, &change.commit_sha)))
}

fn row_of(
    change: &ChangeData, columns: &[ChangeColumn], live: &BTreeMap<String, Vec<Live>>,
    message: Option<String>,
) -> ChangeRow {
    let mut cells = vec![cell(BUILD_COLUMN, latest(change.builds.iter()), false)];
    for column in columns.iter().skip(1) {
        let runs = change
            .deploys
            .iter()
            .filter(|run| run.group_key == column.key);
        cells.push(cell(
            &column.key,
            latest(runs),
            is_live(live, &column.key, change),
        ));
    }
    let deployed = columns
        .iter()
        .skip(1)
        .filter_map(|column| change.first_deploy(&column.key))
        .max_by_key(|run| (run.succeeded_at(), run.run_number));
    ChangeRow {
        commit_sha: change.commit_sha.clone(),
        commit_short: short(&change.commit_sha),
        message,
        cells,
        segments: segments(change.first_green_build(), deployed),
    }
}

impl CorrelationService {
    async fn message_of(&self, change: &ChangeData) -> Option<String> {
        let repo = repo_id(&change.repo)?;
        let sha = CommitSha::parse(&change.commit_sha)?;
        let info = self.commit_info(&repo, sha.as_str()).await?;
        info.message.lines().next().map(str::to_string)
    }

    pub async fn changes(
        &self, id: i64, cursor: Option<String>, limit: usize,
    ) -> DomainResult<ChangesPage> {
        let offset = match cursor {
            Some(cursor) => cursor
                .parse::<usize>()
                .map_err(|_| DomainError::InvalidConfig(views::invalid_cursor()))?,
            None => 0,
        };
        let limit = limit.clamp(1, MAX_LIMIT);
        let facts = self.facts(Some(id)).await?;
        let state = facts
            .states
            .first()
            .ok_or_else(|| DomainError::NotFound(copy::application_not_found(id)))?;
        let live = View::new(&facts, state).live();
        let mut columns = vec![ChangeColumn {
            key: BUILD_COLUMN.to_string(),
            label: views::column_build(),
        }];
        columns.extend(
            ordered_groups(state)
                .into_iter()
                .filter(|g| !g.short_lived)
                .map(|g| ChangeColumn {
                    key: g.key.clone(),
                    label: g.label.clone(),
                }),
        );
        let fetch = i64::try_from(limit + 1).unwrap_or(i64::MAX);
        let skip = i64::try_from(offset).unwrap_or(i64::MAX);
        let mut page = self.changes_page(id, fetch, skip).await?;
        let has_more = page.len() > limit;
        page.truncate(limit);
        let messages = chunked(&page, MESSAGE_PARALLELISM, |change| self.message_of(change)).await;
        let rows = page
            .iter()
            .zip(messages)
            .map(|(change, message)| row_of(change, &columns, &live, message))
            .collect();
        Ok(ChangesPage {
            columns,
            rows,
            has_more,
            next_cursor: has_more.then(|| (offset + limit).to_string()),
        })
    }
}
