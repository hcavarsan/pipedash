import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { TriggerParams } from '../types'

export function useWorkflowParameters(providerId: number, pipelineId: string) {
  return useQuery({
    queryKey: queryKeys.workflows.parameters(providerId, pipelineId),
    queryFn: () => service.getWorkflowParameters(pipelineId),
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    enabled: !!providerId && !!pipelineId,
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

      notifications.show({
        title: 'Workflow Triggered',
        message: 'Workflow has been triggered successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Trigger Workflow',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}
