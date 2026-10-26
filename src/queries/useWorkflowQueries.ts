import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { TriggerParams } from '../types'
import { errorMessage } from '../utils/errorDisplay'

export function useWorkflowParameters(providerId: number, pipelineId: string) {
  return useQuery({
    queryKey: queryKeys.workflows.parameters(providerId, pipelineId),
    queryFn: () => service.getWorkflowParameters(pipelineId),
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    enabled: !!providerId && !!pipelineId,
    retry: false,
  })
}

export function useTriggerWorkflow() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (params: TriggerParams) => service.triggerPipeline(params),

    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(variables.workflow_id),
      })
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      notifications.show({
        title: variables.workflow_id.startsWith('fluxcd__') ? 'Reconciliation requested' : 'Workflow triggered',
        message: variables.workflow_id.startsWith('fluxcd__')
          ? 'Flux will reconcile the resource using its current configuration.'
          : 'Workflow has been triggered successfully',
        color: 'green',
      })
    },

    onError: (error: Error, variables) => {
      notifications.show({
        title: variables.workflow_id.startsWith('fluxcd__')
          ? "Couldn't request reconciliation"
          : "Couldn't trigger workflow",
        message: errorMessage(error, 'Check your provider connection and try again.'),
        color: 'red',
      })
    },
  })
}
