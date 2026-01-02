import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { Pipeline, ProviderSummary } from '../types'

export function usePipelines(
  providerId?: number,
  options?: { enabled?: boolean; providers?: ProviderSummary[] }
) {
  const hasProviders = (options?.providers?.length ?? 0) > 0

  return useQuery({
    queryKey: queryKeys.pipelines.list({ providerId }),
    queryFn: async () => {
      if (!hasProviders && providerId === undefined) {
        logger.debug('usePipelines', 'Skipping fetch - no providers configured')
        
return []
      }

      logger.debug('usePipelines', 'Fetching pipelines', { providerId: providerId ?? 'all' })
      const data = await service.getCachedPipelines(providerId)

      logger.debug('usePipelines', 'Received pipelines', { count: data.length })

      return data
    },
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: (options?.enabled ?? true) && hasProviders,
  })
}

export function useGetPipelinesFromCache() {
  const queryClient = useQueryClient()

  return (providerId?: number) => {
    return queryClient.getQueryData<Pipeline[]>(
      queryKeys.pipelines.list({ providerId })
    )
  }
}

export function useSetPipelinePinned() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ pipelineId, pinned }: { pipelineId: string; pinned: boolean }) => {
      logger.debug('useSetPipelinePinned', 'Setting pipeline pinned', { pipelineId, pinned })
      await service.setPipelinePinned(pipelineId, pinned)
    },
    onMutate: async ({ pipelineId, pinned }) => {
      // Optimistic update - update local cache immediately
      await queryClient.cancelQueries({ queryKey: queryKeys.pipelines.all })

      // Get all pipeline list queries and update them optimistically
      queryClient.setQueriesData<Pipeline[]>(
        { queryKey: queryKeys.pipelines.all },
        (old) => {
          if (!old) return old
          return old.map((p) =>
            p.id === pipelineId ? { ...p, pinned } : p
          )
        }
      )
    },
    onError: (error, variables) => {
      logger.error('useSetPipelinePinned', 'Failed to set pipeline pinned', {
        pipelineId: variables.pipelineId,
        error,
      })
      // Revert on error by refetching
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
    },
  })
}
