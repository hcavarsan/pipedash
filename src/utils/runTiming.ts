import type { PipelineRun } from '../types/pipelines'

import { formatDuration } from './formatDuration'

export const isLiveRun = (run: Pick<PipelineRun, 'pace'>): boolean => run.pace.live || run.pace.queued

function formatQueueWait(pace: PipelineRun['pace'], text: string): string {
  const limit = pace.queue_limit_seconds

  return pace.stuck && limit ? `${text} in queue · past ${formatDuration(limit)} limit` : `${text} in queue`
}

export function formatLiveRunDuration(run: Pick<PipelineRun, 'started_at' | 'pace'>, now: number): string {
  const text = formatDuration(Math.max(0, (now - Date.parse(run.started_at)) / 1000))
  const typical = run.pace.typical_duration_seconds

  if (run.pace.queued) {
    return formatQueueWait(run.pace, text)
  }

  if (!typical) {
    return text
  }

  return `${text} · ${run.pace.past_usual ? 'past usual' : 'usually'} ${formatDuration(typical)}`
}
