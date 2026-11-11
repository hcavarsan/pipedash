use pipedash_plugin_api::{
    ChangedFile,
    CommitComparison,
    CommitInfo,
    CommitSha,
    PluginError,
    PluginResult,
};

use crate::types;

pub(crate) fn commit_info(commit: types::Commit) -> PluginResult<CommitInfo> {
    let sha = CommitSha::parse(&commit.id).ok_or_else(|| {
        PluginError::ApiError(format!(
            "GitLab returned an invalid commit id: {}",
            commit.id
        ))
    })?;
    Ok(CommitInfo {
        sha,
        authored_at: commit.authored_date,
        committed_at: commit.committed_date,
        message: commit.message,
        parents: commit
            .parent_ids
            .iter()
            .filter_map(|id| CommitSha::parse(id))
            .collect(),
    })
}

pub(crate) fn comparison(
    ahead: &[types::CommitRef], behind: &[types::CommitRef],
) -> CommitComparison {
    CommitComparison {
        ahead: u32::try_from(ahead.len()).unwrap_or(u32::MAX),
        behind: u32::try_from(behind.len()).unwrap_or(u32::MAX),
        commits: ahead
            .iter()
            .filter_map(|commit| CommitSha::parse(&commit.id))
            .collect(),
    }
}

pub(crate) fn changed_file(file: types::FileDiff) -> ChangedFile {
    ChangedFile {
        path: file.new_path,
        patch: Some(file.diff).filter(|diff| !diff.is_empty()),
    }
}
