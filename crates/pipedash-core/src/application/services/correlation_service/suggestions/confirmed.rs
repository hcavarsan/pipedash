use serde_json::json;

use super::built_from;
use crate::application::services::correlation_service::applications::Plan;
use crate::application::services::correlation_service::edges::EdgeSpec;
use crate::application::services::correlation_service::vocab::{
    parse_commit_node,
    USER_CONFIRMED,
};
use crate::domain::application::Confidence;

pub(super) fn contains(plan: &Plan, digest: &str, commit_node: &str) -> bool {
    plan.confirmed
        .get(digest)
        .is_some_and(|nodes| nodes.contains(commit_node))
}

pub(super) fn apply(plan: &Plan, digest: &str, desired: &mut Vec<EdgeSpec>) {
    let Some(nodes) = plan.confirmed.get(digest) else {
        return;
    };
    desired.retain(|spec| spec.confidence == Confidence::Exact || !nodes.contains(&spec.to_id));
    for (repo, sha) in nodes.iter().filter_map(|node| parse_commit_node(node)) {
        let evidence = json!({ "repo": repo, "commit": sha });
        let spec = built_from(
            digest,
            repo,
            sha,
            USER_CONFIRMED,
            Confidence::Exact,
            evidence,
        );
        desired.push(spec);
    }
}
