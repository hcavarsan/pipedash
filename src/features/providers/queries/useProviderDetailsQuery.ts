import { type UseQueryResult, useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '@/lib/cacheConfig'
import { queryKeys } from '@/lib/queryKeys'
import { service } from '@/services/service'
import type { ProviderConfig } from '@/types/domain'

export function useProviderDetails(providerId: number | null): UseQueryResult<ProviderConfig & { id: number }, Error> {
  return useQuery({
    queryKey: queryKeys.providers.detail(providerId || 0),
    queryFn: async () => {
      if (!providerId) {
        throw new Error('providerId is required')
      }

      const config = await service.getProvider(providerId)

      return { ...config, id: providerId }
    },
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
    enabled: !!providerId && providerId > 0,
  })
}
