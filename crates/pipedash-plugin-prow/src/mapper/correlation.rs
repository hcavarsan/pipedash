use pipedash_plugin_api::{
    CommitSha,
    Correlation,
    Observed,
    RepoId,
    RunEvidence,
    SourceObservation,
};

use crate::types::{
    ProwJob,
    Refs,
};

pub(crate) fn pipeline_correlation(job: &ProwJob) -> Correlation {
    Correlation {
        sources: sources(job, false),
        ..Correlation::default()
    }
}

pub(crate) fn run_correlation(job: &ProwJob) -> Correlation {
    Correlation {
        sources: sources(job, true),
        ..Correlation::default()
    }
}

pub(crate) fn timing_evidence(timed: bool) -> RunEvidence {
    RunEvidence {
        timing: if timed {
            Observed::Recorded
        } else {
            Observed::Absent
        },
        ..RunEvidence::default()
    }
}

fn sources(job: &ProwJob, with_commit: bool) -> Vec<SourceObservation> {
    job.spec
        .refs
        .iter()
        .chain(&job.spec.extra_refs)
        .filter_map(|refs| {
            let repo = RepoId::parse(refs.repo_link.as_deref()?)?;
            let commit = if with_commit { commit_of(refs) } else { None };
            Some(SourceObservation {
                repo,
                path_scopes: Vec::new(),
                commit,
            })
        })
        .collect()
}

fn commit_of(refs: &Refs) -> Option<CommitSha> {
    let pull_sha = refs
        .pulls
        .first()
        .map(|pull| pull.sha.trim())
        .filter(|sha| !sha.is_empty());
    CommitSha::parse(pull_sha.unwrap_or(&refs.base_sha))
}
