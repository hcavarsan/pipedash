import type { PipelineRun } from '../types/pipelines'

import { formatDuration } from './formatDuration'

export const isLiveRun = (run: Pick<PipelineRun, 'pace'>): boolean => run.pace.live

export function formatLiveRunDuration(run: Pick<PipelineRun, 'started_at' | 'pace'>, now: number): string {
  const text = formatDuration(Math.max(0, (now - Date.parse(run.started_at)) / 1000))
  const typical = run.pace.typical_duration_seconds

  if (!typical) {
    return text
  }

  return `${text} · ${run.pace.past_usual ? 'past usual' : 'usually'} ${formatDuration(typical)}`
}
