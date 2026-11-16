use pipedash_plugin_api::{
    CommitSha,
    Correlation,
    RepoId,
    SourceObservation,
};

use crate::types;

const GIT_BUILD_DATA: &str = "hudson.plugins.git.util.BuildData";

pub(crate) fn build_correlation(build: &types::Build) -> Correlation {
    let sources = build
        .actions
        .iter()
        .filter(|action| action._class.as_deref() == Some(GIT_BUILD_DATA))
        .filter_map(|action| {
            let repo = RepoId::parse(action.remote_urls.first()?)?;
            let commit = action
                .last_built_revision
                .as_ref()
                .and_then(|revision| revision.sha1.as_deref())
                .and_then(CommitSha::parse);
            Some(SourceObservation {
                repo,
                path_scopes: Vec::new(),
                commit,
            })
        })
        .collect();
    Correlation {
        sources,
        ..Correlation::default()
    }
}
