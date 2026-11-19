import type { StatusKey } from '../theme/status'
import type { Pipeline, PipelineRun } from '../types/pipelines'

export const getPipelineStatusLabel = (
  pipeline: Pick<Pipeline, 'overdue_run' | 'stuck_queue_run' | 'resource_state'>
): string | undefined => {
  if (pipeline.overdue_run) {
    return 'Running longer than usual'
  }

  return pipeline.stuck_queue_run ? 'Stuck in queue' : pipeline.resource_state?.label
}

export const pipelineStatusKey = (pipeline: Pick<Pipeline, 'status' | 'last_run' | 'resource_state'>): StatusKey =>
  pipeline.status === 'pending' && !pipeline.last_run && !pipeline.resource_state ? 'never' : pipeline.status

export const getRunStatusLabel = (run: Pick<PipelineRun, 'metadata'>): string | undefined => {
  const label = run.metadata?.status_label

  if (typeof label !== 'string') {
    return undefined
  }

  return run.metadata?.archived === true ? `${label} (cached)` : label
}
