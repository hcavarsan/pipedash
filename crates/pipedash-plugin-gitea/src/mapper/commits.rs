use pipedash_plugin_api::{
    ChangedFile,
    CommitComparison,
    CommitInfo,
    CommitSha,
    PluginError,
    PluginResult,
};

use crate::types;

pub(crate) fn commit_info(commit: &types::Commit) -> PluginResult<CommitInfo> {
    let sha = CommitSha::parse(&commit.sha).ok_or_else(|| {
        PluginError::ApiError(format!(
            "Gitea returned an invalid commit id: {}",
            commit.sha
        ))
    })?;
    Ok(CommitInfo {
        sha,
        authored_at: commit.commit.author.date,
        committed_at: commit.commit.committer.date,
        message: commit.commit.message.clone(),
        parents: commit
            .parents
            .iter()
            .filter_map(|parent| CommitSha::parse(&parent.sha))
            .collect(),
    })
}

pub(crate) fn comparison(ahead: &types::Compare, behind: &types::Compare) -> CommitComparison {
    CommitComparison {
        ahead: u32::try_from(ahead.total_commits).unwrap_or(u32::MAX),
        behind: u32::try_from(behind.total_commits).unwrap_or(u32::MAX),
        commits: ahead
            .commits
            .iter()
            .filter_map(|commit| CommitSha::parse(&commit.sha))
            .collect(),
    }
}

pub(crate) fn changed_files(commit: types::Commit) -> Vec<ChangedFile> {
    commit
        .files
        .into_iter()
        .map(|file| ChangedFile {
            path: file.filename,
            patch: None,
        })
        .collect()
}
