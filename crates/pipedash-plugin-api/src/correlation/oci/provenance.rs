use serde_json::Value;

use super::SlsaProvenance;
use crate::correlation::ids::{
    CommitSha,
    RepoId,
};

pub(super) fn provenance_v1(predicate: &Value) -> SlsaProvenance {
    let definition = &predicate["buildDefinition"];
    SlsaProvenance {
        repo: pointer_str(definition, "/externalParameters/workflow/repository")
            .and_then(RepoId::parse),
        commit: definition["resolvedDependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|dependency| CommitSha::parse(pointer_str(dependency, "/digest/gitCommit")?)),
        invocation: pointer_str(predicate, "/runDetails/metadata/invocationId").map(str::to_string),
    }
}

pub(super) fn provenance_v02(predicate: &Value) -> SlsaProvenance {
    SlsaProvenance {
        repo: pointer_str(predicate, "/invocation/configSource/uri")
            .and_then(|uri| RepoId::parse(source_url(uri))),
        commit: pointer_str(predicate, "/invocation/configSource/digest/sha1")
            .and_then(CommitSha::parse),
        invocation: pointer_str(predicate, "/metadata/buildInvocationId").map(str::to_string),
    }
}

fn pointer_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer)?.as_str()
}

pub(super) fn source_url(uri: &str) -> &str {
    let uri = uri.strip_prefix("git+").unwrap_or(uri);
    let Some((scheme, rest)) = uri.split_once("://") else {
        return uri;
    };
    let path_start = rest.find('/').unwrap_or(rest.len());
    match rest[path_start..].find('@') {
        Some(at) => &uri[..scheme.len() + 3 + path_start + at],
        None => uri,
    }
}
