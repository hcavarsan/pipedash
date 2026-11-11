use serde::{
    Deserialize,
    Serialize,
};

use super::pipeline::{
    Pipeline,
    PipelineRun,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunTarget {
    pub pipeline_id: String,
    pub run_number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelGroup {
    pub pipeline: Pipeline,
    pub provider_name: String,
    pub runs: Vec<PipelineRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedPipeline {
    pub pipeline_id: String,
    pub provider_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CancellationTargets {
    pub groups: Vec<CancelGroup>,
    pub batches: Vec<Vec<RunTarget>>,
    pub failed_pipeline_ids: Vec<String>,
    pub skipped: Vec<SkippedPipeline>,
    pub run_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationResult {
    pub target: RunTarget,
    pub pipeline_name: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationBatch {
    pub results: Vec<CancellationResult>,
    pub requested_count: usize,
    pub failed_count: usize,
}
