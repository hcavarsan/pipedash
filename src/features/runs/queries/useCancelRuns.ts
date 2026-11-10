import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query'

import { notifications } from '../../../lib/notifications'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { CancellationBatch, RunTarget } from '../../../types/run_action_batch'
import { summarizeCancellation } from '../../../utils/cancelActiveRuns'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { invalidateRunHistory } from '../lib/runHistoryCache'

export function useCancelRuns(): UseMutationResult<CancellationBatch, Error, RunTarget[][], unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (batches: RunTarget[][]) => {
      const combined: CancellationBatch = { results: [], requested_count: 0, failed_count: 0 }
      for (const targets of batches) {
        const batch = await service.cancelPipelineRuns(targets)
        combined.results.push(...batch.results)
        combined.requested_count += batch.requested_count
        combined.failed_count += batch.failed_count
        for (const { target, error } of batch.results) {
          if (error === null) {
            void invalidateRunHistory(queryClient, target.pipeline_id, target.run_number)
          }
        }
        for (const pipelineId of new Set(batch.results.map(({ target }) => target.pipeline_id))) {
          void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })
        }
      }
      return combined
    },

    onSuccess: (batch) => notifications.show(summarizeCancellation(batch)),

    onError: (error) => displayErrorNotification(error, "Couldn't cancel runs"),
  })
}
