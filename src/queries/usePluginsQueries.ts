import { type UseQueryResult, useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services/service'
import type { PluginPresentation } from '../types/plugin_presentation'
import type { PluginMetadata } from '../types/plugins'

export function usePlugins(): UseQueryResult<PluginMetadata[], Error> {
  return useQuery({
    queryKey: queryKeys.plugins.list(),
    queryFn: async () => {
      logger.debug('usePlugins', 'Fetching plugins...')
      const data = await service.getAvailablePlugins()

      logger.debug('usePlugins', 'Received plugins', { count: data.length })

      return data
    },
    staleTime: STALE_TIMES.STATIC,
    gcTime: GC_TIMES.LONG,
  })
}

export function usePluginPresentation(providerType: string | undefined): PluginPresentation | undefined {
  return usePlugins().data?.find((plugin) => plugin.provider_type === providerType)?.presentation
}
