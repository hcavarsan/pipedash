import { useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '@/lib/cacheConfig'
import { queryKeys } from '@/lib/queryKeys'
import { service } from '@/services'

export function useProviderDetails(providerId: number | null) {
  return useQuery({
    queryKey: queryKeys.providers.detail(providerId || 0),
    queryFn: async () => {
      const config = await service.getProvider(providerId!)



return { ...config, id: providerId! }
    },
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
    enabled: !!providerId && providerId > 0,
  })
}
