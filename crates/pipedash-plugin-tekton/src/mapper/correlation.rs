use pipedash_plugin_api::{
    ArtifactObservation,
    CommitSha,
    Correlation,
    Digest,
    ImageRef,
    RepoId,
    SourceObservation,
};

use crate::types::{
    TaskResult,
    TektonPipelineRun,
    TektonTaskRun,
};

const PAC_REPO_URL: &str = "pipelinesascode.tekton.dev/repo-url";
const PAC_SHA: &str = "pipelinesascode.tekton.dev/sha";
const GIT_REVISION_LABEL: &str = "tekton.dev/gitRevision";
const URL_PARAMS: [&str; 3] = ["repo-url", "git-url", "url"];

pub(crate) fn pipeline_correlation(run: &TektonPipelineRun) -> Correlation {
    Correlation {
        sources: source(run).into_iter().collect(),
        ..Correlation::default()
    }
}

pub(crate) fn run_correlation(run: &TektonPipelineRun, tasks: &[TektonTaskRun]) -> Correlation {
    Correlation {
        produced: produced(run, tasks),
        ..pipeline_correlation(run)
    }
}

fn source(run: &TektonPipelineRun) -> Option<SourceObservation> {
    let meta = &run.metadata;
    let (repo, commit) = match meta.annotations.get(PAC_REPO_URL) {
        Some(url) => (RepoId::parse(url)?, meta.annotations.get(PAC_SHA)),
        None => {
            let repo = run
                .spec
                .params
                .iter()
                .find(|param| URL_PARAMS.contains(&param.name.as_str()))
                .and_then(|param| param.value.as_str())
                .and_then(RepoId::parse)?;
            (repo, meta.labels.get(GIT_REVISION_LABEL))
        }
    };
    Some(SourceObservation {
        repo,
        path_scopes: Vec::new(),
        commit: commit.map(String::as_str).and_then(CommitSha::parse),
    })
}

fn produced(run: &TektonPipelineRun, tasks: &[TektonTaskRun]) -> Vec<ArtifactObservation> {
    let embedded = run
        .status
        .task_runs
        .values()
        .filter_map(|task| task.status.as_ref())
        .map(|status| status.results.as_slice());
    let mut found: Vec<ArtifactObservation> = std::iter::once(run.status.results.as_slice())
        .chain(tasks.iter().map(|task| task.status.results.as_slice()))
        .chain(embedded)
        .flat_map(result_images)
        .collect();
    found.sort_by(|a, b| {
        (&a.repository, &a.digest, &a.tag).cmp(&(&b.repository, &b.digest, &b.tag))
    });
    found.dedup();
    found
}

fn result_value<'a>(results: &'a [TaskResult], name: &str) -> Option<&'a str> {
    results
        .iter()
        .find(|result| result.name == name)?
        .value
        .as_str()
}

fn result_images(results: &[TaskResult]) -> Vec<ArtifactObservation> {
    let single = result_value(results, "IMAGE_URL")
        .and_then(ImageRef::parse)
        .map(|image| ImageRef {
            digest: result_value(results, "IMAGE_DIGEST")
                .and_then(Digest::parse)
                .or(image.digest),
            ..image
        });
    let listed = result_value(results, "IMAGES")
        .into_iter()
        .flat_map(|images| images.split([',', '\n']))
        .filter_map(ImageRef::parse);
    single
        .into_iter()
        .chain(listed)
        .filter(|image| image.digest.is_some())
        .map(ImageRef::into_observation)
        .collect()
}
