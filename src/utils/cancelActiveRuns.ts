import type { CancellationBatch } from '../types/run_action_batch'
import { formatCount } from './formatCount'

export interface CancellationSummary {
  title: string
  message: string
  failed: boolean
}

export const summarizeCancellation = ({
  results,
  requested_count,
  failed_count,
}: CancellationBatch): CancellationSummary => {
  if (failed_count === 0) {
    return {
      title: 'Cancellation requested',
      message: `Cancellation requested for ${formatCount(requested_count, 'run')}`,
      failed: false,
    }
  }

  const failures = results
    .filter((result) => result.error !== null)
    .map((result) => `${result.pipeline_name} #${result.target.run_number} (${result.error})`)

  return {
    title: "Couldn't cancel every run",
    message: `Cancellation requested for ${requested_count - failed_count} of ${formatCount(requested_count, 'run')}. Failed: ${failures.join('; ')}`,
    failed: true,
  }
}
