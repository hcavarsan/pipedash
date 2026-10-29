import type { PipelineRun } from '../types/domain'

import { formatDuration } from './formatDuration'

export const isLiveRun = (run: Pick<PipelineRun, 'status' | 'started_at'>): boolean =>
  run.status === 'running' && Number.isFinite(Date.parse(run.started_at))

export function formatLiveRunDuration(
  run: Pick<PipelineRun, 'started_at'>,
  now: number,
  typicalSeconds?: number | null
): string {
  const elapsed = Math.max(0, (now - Date.parse(run.started_at)) / 1000)
  const text = formatDuration(elapsed)

  if (!typicalSeconds || typicalSeconds <= 0) {
    return text
  }

  return `${text} · ${elapsed > typicalSeconds ? 'past usual' : 'usually'} ${formatDuration(typicalSeconds)}`
}
