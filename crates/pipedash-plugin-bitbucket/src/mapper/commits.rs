use std::collections::HashMap;

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
    let sha = CommitSha::parse(&commit.hash).ok_or_else(|| {
        PluginError::ApiError(format!(
            "Bitbucket returned an invalid commit hash: {}",
            commit.hash
        ))
    })?;
    Ok(CommitInfo {
        sha,
        authored_at: commit.date,
        committed_at: commit.date,
        message: commit.message.clone(),
        parents: commit
            .parents
            .iter()
            .filter_map(|parent| CommitSha::parse(&parent.hash))
            .collect(),
    })
}

pub(crate) fn comparison(
    ahead: &[types::CommitHash], behind: &[types::CommitHash],
) -> CommitComparison {
    CommitComparison {
        ahead: u32::try_from(ahead.len()).unwrap_or(u32::MAX),
        behind: u32::try_from(behind.len()).unwrap_or(u32::MAX),
        commits: ahead
            .iter()
            .filter_map(|commit| CommitSha::parse(&commit.hash))
            .collect(),
    }
}

pub(crate) fn changed_files(stats: &[types::DiffStat], diff: &str) -> Vec<ChangedFile> {
    let patches = split_patches(diff);
    stats
        .iter()
        .filter_map(|stat| stat.new.as_ref().or(stat.old.as_ref()))
        .map(|file| ChangedFile {
            path: file.path.clone(),
            patch: patches.get(file.path.as_str()).map(ToString::to_string),
        })
        .collect()
}

fn split_patches(diff: &str) -> HashMap<&str, &str> {
    let mut starts: Vec<usize> = diff
        .match_indices("diff --git ")
        .map(|(index, _)| index)
        .filter(|&index| index == 0 || diff[..index].ends_with('\n'))
        .collect();
    starts.push(diff.len());

    starts
        .windows(2)
        .filter_map(|bounds| {
            let chunk = &diff[bounds[0]..bounds[1]];
            let patch = &chunk[chunk.find("\n@@")? + 1..];
            Some((chunk_path(chunk)?, patch))
        })
        .collect()
}

fn chunk_path(chunk: &str) -> Option<&str> {
    let header = || chunk.lines().take_while(|line| !line.starts_with("@@"));
    header()
        .find_map(|line| line.strip_prefix("+++ b/"))
        .or_else(|| header().find_map(|line| line.strip_prefix("--- a/")))
}
