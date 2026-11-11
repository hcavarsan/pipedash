import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query'
import { useRef } from 'react'

import { notifications } from '../../../lib/notifications'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { CancellationBatch, RunTarget } from '../../../types/run_action_batch'
import { summarizeCancellation } from '../../../utils/cancelActiveRuns'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { invalidateRunHistory } from '../lib/runHistoryCache'

interface CancellationProgress {
  batches: RunTarget[][]
  nextBatch: number
  combined: CancellationBatch
}

export function useCancelRuns(): UseMutationResult<CancellationBatch, Error, RunTarget[][], unknown> {
  const queryClient = useQueryClient()
  const progress = useRef<CancellationProgress | null>(null)

  const mutation = useMutation({
    mutationFn: async (batches: RunTarget[][]) => {
      if (progress.current?.batches !== batches) {
        progress.current = {
          batches,
          nextBatch: 0,
          combined: { results: [], requested_count: 0, failed_count: 0 },
        }
      }
      const current = progress.current
      const { combined } = current
      for (const [index, targets] of batches.entries()) {
        if (index < current.nextBatch) {
          continue
        }
        const batch = await service.cancelPipelineRuns(targets)
        combined.results.push(...batch.results)
        combined.requested_count += batch.requested_count
        combined.failed_count += batch.failed_count
        current.nextBatch = index + 1
        for (const { target, error } of batch.results) {
          if (error === null) {
            void invalidateRunHistory(queryClient, target.pipeline_id, target.run_number)
          }
        }
        for (const pipelineId of new Set(batch.results.map(({ target }) => target.pipeline_id))) {
          void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })
        }
      }
      progress.current = null
      return combined
    },

    onSuccess: (batch) => notifications.show(summarizeCancellation(batch)),

    onError: (error) => displayErrorNotification(error, "Couldn't cancel runs"),
  })

  return {
    ...mutation,
    reset: () => {
      progress.current = null
      mutation.reset()
    },
  }
}
