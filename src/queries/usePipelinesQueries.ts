import { useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { ProviderSummary } from '../types'

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
