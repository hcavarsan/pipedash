import { type QueryClient, useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { Pipeline, ProviderSummary } from '../types'

export function invalidateProviderPipelines(queryClient: QueryClient, providerId: number) {
  queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.list({ providerId }), exact: true })
  queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.list(), exact: true })
}

export function setProviderPipelines(queryClient: QueryClient, providerId: number, pipelines: Pipeline[]) {
  const allKey = queryKeys.pipelines.list()

  queryClient.setQueryData(queryKeys.pipelines.list({ providerId }), pipelines)

  if (queryClient.getQueryData(allKey) === undefined) {
    queryClient.invalidateQueries({ queryKey: allKey, exact: true })

    return
  }

  queryClient.setQueryData<Pipeline[]>(allKey, (previous = []) => [
    ...previous.filter((pipeline) => pipeline.provider_id !== providerId),
    ...pipelines,
  ])
}

export function usePipelines(providerId?: number, options?: { enabled?: boolean; providers?: ProviderSummary[] }) {
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
