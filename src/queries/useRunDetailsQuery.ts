import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '@/lib/cacheConfig'
import { queryKeys } from '@/lib/queryKeys'
import { service } from '@/services'
import type { PipelineRun } from '@/types'
import { displayErrorNotification, displaySuccessNotification } from '@/utils/errorDisplay'

export function useRunDetails(
  pipelineId: string,
  runNumber: number,
  enabled: boolean = true
) {
  return useQuery({
    queryKey: queryKeys.runs.detail(pipelineId, runNumber),
    queryFn: () => service.getWorkflowRunDetails(pipelineId, runNumber),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: enabled && !!pipelineId && runNumber > 0,
    refetchInterval: (query) => {
      const data = query.state.data as PipelineRun | undefined
      const isRunning = data?.status === 'running' || data?.status === 'pending'



return isRunning ? 10000 : false
    },
    refetchIntervalInBackground: false,
  })
}

async function pollForRunAvailability(
  pipelineId: string,
  runNumber: number,
  maxRetries: number = 5
): Promise<void> {
  let retries = maxRetries

  while (retries > 0) {
    await new Promise((resolve) => setTimeout(resolve, 2000))

    try {
      const runDetails = await service.getWorkflowRunDetails(pipelineId, runNumber)


      if (runDetails?.status) {
        return
      }
    } catch {
return
    }

    retries--
  }

  throw new Error('Run not available after polling')
}

export function useRerunWorkflow() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({
      pipelineId,
      inputs,
    }: {
      pipelineId: string
      inputs?: Record<string, any>
    }) => {
      const result = await service.triggerPipeline({
        workflow_id: pipelineId,
        inputs,
      })

      let newRunNumber = 0


      try {
        const parsed = JSON.parse(result)


        newRunNumber = parsed.run_number || parsed.build_number || parsed.number || 0
      } catch {
        console.warn('Could not parse trigger response:', result)
      }

      if (newRunNumber > 0) {
        await pollForRunAvailability(pipelineId, newRunNumber, 5)
      }

      return { result, newRunNumber }
    },

    onSuccess: ({ newRunNumber }, { pipelineId }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

      if (newRunNumber > 0) {
        queryClient.prefetchQuery({
          queryKey: queryKeys.runs.detail(pipelineId, newRunNumber),
          queryFn: () => service.getWorkflowRunDetails(pipelineId, newRunNumber),
        })
      }

      displaySuccessNotification('Workflow re-run started successfully', 'Workflow Triggered')
    },

    onError: (error: Error) => {
      displayErrorNotification(error, 'Failed to Re-run Workflow')
    },
  })
}
