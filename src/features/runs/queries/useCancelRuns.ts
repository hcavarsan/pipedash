import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query'

import { notifications } from '../../../lib/notifications'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { CancellationBatch, RunTarget } from '../../../types/run_action_batch'
import { summarizeCancellation } from '../../../utils/cancelActiveRuns'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { invalidateRunHistory } from '../lib/runHistoryCache'

export function useCancelRuns(): UseMutationResult<CancellationBatch, Error, RunTarget[], unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (targets: RunTarget[]) => service.cancelPipelineRuns(targets),

    onSuccess: ({ results, ...batch }) => {
      for (const { target, error } of results) {
        if (error === null) {
          void invalidateRunHistory(queryClient, target.pipeline_id, target.run_number)
        }
      }

      for (const pipelineId of new Set(results.map(({ target }) => target.pipeline_id))) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })
      }

      notifications.show(summarizeCancellation({ results, ...batch }))
    },

    onError: (error) => displayErrorNotification(error, "Couldn't cancel runs"),
  })
}
