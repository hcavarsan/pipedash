import type { StatusKey } from '../theme/status'
import type { Pipeline, PipelineRun } from '../types/pipelines'

export const getPipelineStatusLabel = (
  pipeline: Pick<Pipeline, 'overdue_run' | 'resource_state'>
): string | undefined => (pipeline.overdue_run ? 'Running longer than usual' : pipeline.resource_state?.label)

export const pipelineStatusKey = (pipeline: Pick<Pipeline, 'status' | 'last_run' | 'resource_state'>): StatusKey =>
  pipeline.status === 'pending' && !pipeline.last_run && !pipeline.resource_state ? 'never' : pipeline.status

export const getRunStatusLabel = (run: Pick<PipelineRun, 'metadata'>): string | undefined => {
  const label = run.metadata?.status_label

  if (typeof label !== 'string') {
    return undefined
  }

  return run.metadata?.archived === true ? `${label} (cached)` : label
}
