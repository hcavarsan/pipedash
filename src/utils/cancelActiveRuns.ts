import type { NotificationData } from '@mantine/notifications'

import type { CancelGroup, CancellationBatch, RunTarget } from '../types/run_action_batch'

import { formatCount } from './formatCount'

export const runTargetsOf = (groups: CancelGroup[]): RunTarget[] =>
  groups.flatMap(({ pipeline, runs }) => runs.map((run) => ({ pipeline_id: pipeline.id, run_number: run.run_number })))

export const summarizeCancellation = ({
  results,
  requested_count,
  failed_count,
}: CancellationBatch): NotificationData => {
  if (failed_count === 0) {
    return {
      title: 'Cancellation requested',
      message: `Cancellation requested for ${formatCount(requested_count, 'run')}`,
      color: 'gray',
    }
  }

  const failures = results
    .filter((result) => result.error !== null)
    .map((result) => `${result.pipeline_name} #${result.target.run_number} (${result.error})`)

  return {
    title: "Couldn't cancel every run",
    message: `Cancellation requested for ${requested_count - failed_count} of ${formatCount(requested_count, 'run')}. Failed: ${failures.join('; ')}`,
    color: 'red',
  }
}
