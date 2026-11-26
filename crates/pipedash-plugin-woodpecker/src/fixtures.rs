use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use serde_json::json;

use crate::{
    mapper,
    types,
};

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let repo: types::Repo = serde_json::from_value(json!({
        "id": 5,
        "owner": "acme",
        "name": "web",
        "full_name": "acme/web",
        "default_branch": "main",
        "config_file": ".woodpecker.yaml",
        "forge_url": "https://forge.test/acme/web",
        "clone_url": "https://forge.test/acme/web.git"
    }))
    .ok()?;
    let latest: types::Pipeline = serde_json::from_value(json!({
        "id": 1042,
        "number": 42,
        "status": "success",
        "event": "push",
        "branch": "main",
        "commit": "0123456789abcdef0123456789abcdef01234567",
        "message": "change",
        "author": "dev",
        "created": 1_761_645_600,
        "started": 1_761_645_610,
        "finished": 1_761_645_670
    }))
    .ok()?;
    let source = mapper::source_repo(&repo);
    let run = mapper::map_run(
        &latest,
        &repo.id.to_string(),
        "https://ci.test/repos/5/pipeline/42".to_string(),
        source.as_ref(),
    );
    Some((mapper::map_pipeline(&repo, Some(&latest)), vec![run]))
}
