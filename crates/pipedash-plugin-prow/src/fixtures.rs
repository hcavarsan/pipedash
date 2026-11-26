use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};

use crate::config::Selection;
use crate::mapper::{
    map_pipeline,
    map_run,
    Latest,
};
use crate::types::ProwJob;

pub(crate) const PULL_SHA: &str = "0123456789abcdef0123456789abcdef01234567";
pub(crate) const BASE_SHA: &str = "89abcdef0123456789abcdef0123456789abcdef";
pub(crate) const EXTRA_SHA: &str = "fedcba9876543210fedcba9876543210fedcba98";

pub(crate) fn job_json() -> serde_json::Value {
    serde_json::json!({
        "metadata": {"name": "uuid-1", "namespace": "default"},
        "spec": {
            "type": "presubmit",
            "job": "pull-shop-test",
            "refs": {
                "org": "shop",
                "repo": "api",
                "repo_link": "https://github.com/shop/api",
                "base_ref": "main",
                "base_sha": BASE_SHA,
                "pulls": [{"number": 7, "author": "dev", "sha": PULL_SHA}]
            },
            "extra_refs": [
                {
                    "org": "shop",
                    "repo": "config",
                    "repo_link": "https://github.com/shop/config",
                    "base_ref": "main",
                    "base_sha": EXTRA_SHA
                },
                {"org": "shop", "repo": "linkless", "base_sha": EXTRA_SHA}
            ]
        },
        "status": {
            "startTime": "2026-01-01T10:00:00Z",
            "completionTime": "2026-01-01T10:05:00Z",
            "state": "success",
            "build_id": "1234",
            "url": "https://prow.test/view/gs/bucket/pr-logs/pull/shop_api/7/pull-shop-test/1234"
        }
    })
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    let Ok(job) = serde_json::from_value::<ProwJob>(job_json()) else {
        return Vec::new();
    };
    let (Ok(selection), Ok(deck)) = (
        Selection::parse("shop/api/pull-shop-test"),
        reqwest::Url::parse("https://prow.test"),
    ) else {
        return Vec::new();
    };
    let pipeline = map_pipeline(&selection, Latest::Deck(&job), &deck, false);
    let runs = map_run(&job, &pipeline.id, &deck).into_iter().collect();
    vec![(pipeline, runs)]
}
