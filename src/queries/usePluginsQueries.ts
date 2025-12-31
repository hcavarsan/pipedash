import { useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'

export function usePlugins() {
  return useQuery({
    queryKey: queryKeys.plugins.list(),
    queryFn: async () => {
      try {
        logger.debug('usePlugins', 'Fetching plugins...')
        const data = await service.getAvailablePlugins()

        logger.debug('usePlugins', 'Received plugins', { count: data.length })

        return data
      } catch (error: any) {
        if (error?.response?.status === 503) {
          logger.debug('usePlugins', 'App not initialized yet, returning empty array')

return []
        }
        throw error
      }
    },
    staleTime: STALE_TIMES.STATIC,
    gcTime: GC_TIMES.LONG,
    retry: false,
  })
}
