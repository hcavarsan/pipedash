import type { StatusKey } from '../theme/status'
import type { Pipeline, PipelineRun } from '../types/pipelines'
import { formatDateTime } from './formatDateTime'
import { formatDuration } from './formatDuration'

const DAY_SECONDS = 86_400

export const formatGap = (seconds: number): string =>
  seconds >= DAY_SECONDS ? `${Math.round(seconds / DAY_SECONDS)}d` : formatDuration(seconds)

export const isQuietWatched = (
  pipeline: Pick<Pipeline, 'id' | 'cadence'>,
  quietPipelines: readonly string[]
): boolean => Boolean(pipeline.cadence?.quiet) && quietPipelines.includes(pipeline.id)

export const quietDetail = (pipeline: Pick<Pipeline, 'last_run' | 'cadence'>): string | undefined =>
  pipeline.cadence && pipeline.last_run
    ? `No run since ${formatDateTime(pipeline.last_run)}, usually every ${formatGap(pipeline.cadence.usual_gap_seconds)}`
    : undefined

const statusNote = (pipeline: Pick<Pipeline, 'overdue_run'>, quiet: boolean): string | undefined => {
  if (pipeline.overdue_run) {
    return 'Running longer than usual'
  }

  return quiet ? 'Stopped running' : undefined
}

export const getPipelineStatusLabel = (
  pipeline: Pick<Pipeline, 'overdue_run' | 'resource_state'>,
  quiet = false
): string | undefined => statusNote(pipeline, quiet) ?? pipeline.resource_state?.label

export const pipelineStatusKey = (pipeline: Pick<Pipeline, 'status' | 'last_run' | 'resource_state'>): StatusKey =>
  pipeline.status === 'pending' && !pipeline.last_run && !pipeline.resource_state ? 'never' : pipeline.status

export const getRunStatusLabel = (run: Pick<PipelineRun, 'metadata'>): string | undefined => {
  const label = run.metadata?.status_label

  if (typeof label !== 'string') {
    return undefined
  }

  return run.metadata?.archived === true ? `${label} (cached)` : label
}
