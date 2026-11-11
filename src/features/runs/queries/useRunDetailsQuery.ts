import { type UseQueryResult, useQuery, useQueryClient } from '@tanstack/react-query'
import { useRef } from 'react'

import { GC_TIMES, STALE_TIMES } from '@/lib/cacheConfig'
import { queryKeys } from '@/lib/queryKeys'
import { service } from '@/services/service'
import type { PipelineRun } from '@/types/pipelines'

import { useRunUpdatedEvent } from '../hooks/useRunUpdatedEvent'

const CANCELLATION_WATCH_INTERVAL = 1000
const CANCELLATION_WATCH_ATTEMPTS = 10
const HEARTBEAT_INTERVAL = 30_000

export function useRunDetails(
  pipelineId: string,
  runNumber: number,
  enabled: boolean = true,
  cancellationWatch: boolean = false
): UseQueryResult<PipelineRun, Error> {
  const queryClient = useQueryClient()
  const watchStart = useRef<number | null>(null)
  const active = enabled && !!pipelineId && runNumber > 0
  const queryKey = queryKeys.runs.detail(pipelineId, runNumber)

  useRunUpdatedEvent(pipelineId, active, async (run, isCurrent) => {
    if (run.run_number !== runNumber) {
      return
    }

    await queryClient.cancelQueries({ queryKey, exact: true })

    if (isCurrent()) {
      queryClient.setQueryData(queryKey, run)
    }
  })

  return useQuery({
    queryKey,
    queryFn: () => service.getWorkflowRunDetails(pipelineId, runNumber),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: active,
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

      return HEARTBEAT_INTERVAL
    },
    refetchIntervalInBackground: false,
  })
}
