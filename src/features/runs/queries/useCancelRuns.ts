import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query'

import { notifications } from '../../../lib/notifications'
import { service } from '../../../services/service'
import type { CancellationBatch, RunTarget } from '../../../types/run_action_batch'
import { summarizeCancellation } from '../../../utils/cancelActiveRuns'
import { invalidateRunHistory } from '../lib/runHistoryCache'

export function useCancelRuns(): UseMutationResult<CancellationBatch, Error, RunTarget[], unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (targets: RunTarget[]) => service.cancelPipelineRuns(targets),

    onSuccess: ({ results, ...batch }) => {
      for (const { target, error } of results) {
        if (error === null) {
          invalidateRunHistory(queryClient, target.pipeline_id, target.run_number)
        }
      }

      notifications.show(summarizeCancellation({ results, ...batch }))
    },
  })
}
