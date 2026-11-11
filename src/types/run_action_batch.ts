import type { Pipeline, PipelineRun } from './pipelines'

export interface RunTarget {
  pipeline_id: string
  run_number: number
}

export interface CancelGroup {
  pipeline: Pipeline
  provider_name: string
  runs: PipelineRun[]
}

export interface SkippedPipeline {
  pipeline_id: string
  provider_name: string
  reason: string
}

export interface CancellationTargets {
  groups: CancelGroup[]
  failed_pipeline_ids: string[]
  skipped: SkippedPipeline[]
  run_count: number
}

export interface CancellationResult {
  target: RunTarget
  pipeline_name: string
  error: string | null
}

export interface CancellationBatch {
  results: CancellationResult[]
  requested_count: number
  failed_count: number
}
