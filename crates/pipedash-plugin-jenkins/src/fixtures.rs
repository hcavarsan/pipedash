use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};

use crate::mapper::{
    build_status,
    build_to_pipeline_run,
    correlation::build_correlation,
};
use crate::types;

pub(crate) const BUILD_JSON: &str = r#"{
    "number": 7,
    "result": "SUCCESS",
    "building": false,
    "timestamp": 1700000000000,
    "duration": 90000,
    "url": "http://ci.test/job/shop/7/",
    "actions": [
        {"_class": "hudson.model.CauseAction", "causes": [{"shortDescription": "Started by user dev", "userName": "dev"}]},
        {
            "_class": "hudson.plugins.git.util.BuildData",
            "remoteUrls": ["git@Git.Test:Shop/Api.git"],
            "lastBuiltRevision": {
                "SHA1": "0123456789abcdef0123456789abcdef01234567",
                "branch": [{"SHA1": "0123456789abcdef0123456789abcdef01234567", "name": "refs/remotes/origin/main"}]
            }
        }
    ]
}"#;

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    let Ok(build) = serde_json::from_str::<types::Build>(BUILD_JSON) else {
        return Vec::new();
    };
    let pipeline = Pipeline {
        correlation: build_correlation(&build),
        id: "shop".to_string(),
        name: "shop".to_string(),
        status: build_status(&build),
        last_run: None,
        last_updated: Utc::now(),
        repository: "shop".to_string(),
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata: HashMap::new(),
    };
    let run = build_to_pipeline_run(build, "shop", "http://ci.test", "shop");
    vec![(pipeline, vec![run])]
}
