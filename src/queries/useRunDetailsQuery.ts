import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useRef } from 'react'

import { GC_TIMES, STALE_TIMES } from '@/lib/cacheConfig'
import { logger } from '@/lib/logger'
import { queryKeys } from '@/lib/queryKeys'
import { service } from '@/services'
import type { PipelineRun } from '@/types'
import { displayErrorNotification, displaySuccessNotification } from '@/utils/errorDisplay'

const CANCELLATION_WATCH_INTERVAL = 1000
const CANCELLATION_WATCH_ATTEMPTS = 10

export function useRunDetails(
  pipelineId: string,
  runNumber: number,
  enabled: boolean = true,
  cancellationWatch: boolean = false
): UseQueryResult<PipelineRun, Error> {
  const watchStart = useRef<number | null>(null)

  return useQuery({
    queryKey: queryKeys.runs.detail(pipelineId, runNumber),
    queryFn: () => service.getWorkflowRunDetails(pipelineId, runNumber),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: enabled && !!pipelineId && runNumber > 0,
    refetchInterval: (query) => {
      const data = query.state.data
      const isRunning = data?.status === 'running' || data?.status === 'pending'

      if (!isRunning || !cancellationWatch) {
        watchStart.current = null
      }

      if (!isRunning) {
        return false
      }

      if (cancellationWatch) {
        watchStart.current ??= query.state.dataUpdateCount

        if (query.state.dataUpdateCount - watchStart.current < CANCELLATION_WATCH_ATTEMPTS) {
          return CANCELLATION_WATCH_INTERVAL
        }
      }

      return 10000
    },
    refetchIntervalInBackground: false,
  })
}

export function useRerunWorkflow(): UseMutationResult<
  { result: string; newRunNumber: number },
  Error,
  { pipelineId: string; inputs?: Record<string, unknown> | undefined },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['runs', 'rerun'],
    mutationFn: async ({
      pipelineId,
      inputs,
    }: {
      pipelineId: string
      inputs?: Record<string, unknown> | undefined
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
        logger.warn('useRerunWorkflow', 'Could not parse trigger response', result)
      }

      return { result, newRunNumber }
    },

    onSuccess: ({ newRunNumber }, { pipelineId }) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      if (newRunNumber > 0) {
        void queryClient.prefetchQuery({
          queryKey: queryKeys.runs.detail(pipelineId, newRunNumber),
          queryFn: () => service.getWorkflowRunDetails(pipelineId, newRunNumber),
        })
      }

      displaySuccessNotification(
        pipelineId.startsWith('fluxcd__')
          ? 'Flux will reconcile the current resource configuration.'
          : 'Workflow rerun started',
        pipelineId.startsWith('fluxcd__') ? 'Reconciliation requested' : 'Workflow triggered'
      )
    },

    onError: (error: Error, { pipelineId }) => {
      displayErrorNotification(
        error,
        pipelineId.startsWith('fluxcd__') ? "Couldn't request reconciliation" : "Couldn't rerun workflow"
      )
    },
  })
}
