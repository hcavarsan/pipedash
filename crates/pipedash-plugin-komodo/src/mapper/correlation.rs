use pipedash_plugin_api::{
    ClusterId,
    CommitSha,
    Correlation,
    RepoId,
    TargetLifetime,
    TargetObservation,
    UnitId,
};

use crate::types::{
    StackInfo,
    StackListItem,
    Update,
};

fn repo(info: &StackInfo) -> Option<RepoId> {
    let link = info.repo_link.trim();
    let link = Some(info.branch.as_str())
        .filter(|branch| !branch.is_empty())
        .and_then(|branch| link.strip_suffix(branch))
        .and_then(|rest| rest.strip_suffix("/tree/"))
        .unwrap_or(link);
    RepoId::parse(link)
}

fn target(info: &StackInfo) -> Option<TargetObservation> {
    let server = [&info.server_name, &info.server_id]
        .into_iter()
        .find(|name| !name.is_empty())?;
    Some(TargetObservation {
        cluster: ClusterId::from_host(server)?,
        namespace: None,
        name: server.clone(),
        lifetime: TargetLifetime::Standing,
    })
}

pub(super) fn pipeline(stack: &StackListItem) -> Correlation {
    let info = &stack.info;
    Correlation {
        unit: UnitId::new(&stack.id),
        targets: target(info).into_iter().collect(),
        ..Correlation::single_source(
            repo(info),
            info.deployed_hash.as_deref().and_then(CommitSha::parse),
        )
    }
}

pub(super) fn run(info: Option<&StackInfo>, detail: Option<&Update>) -> Correlation {
    Correlation::single_source(
        info.and_then(repo),
        detail.and_then(|update| CommitSha::parse(&update.commit_hash)),
    )
}
