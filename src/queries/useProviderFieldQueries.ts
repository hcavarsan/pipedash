import { useInfiniteQuery, useQuery } from '@tanstack/react-query'

import { PAGE_SIZES } from '../constants/pagination'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { service } from '../services'
import type { PaginatedAvailablePipelines, ProviderConfig } from '../types'

export function useProviderOrganizations(
  providerType: string,
  config: ProviderConfig,
  enableQuery = true
) {
  return useQuery({
    queryKey: ['providerOrganizations', providerType, config.config],
    queryFn: () => service.fetchProviderOrganizations(providerType, config.config || {}),
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
    enabled: enableQuery && !!providerType && !!config.token && Object.keys(config.config || {}).length > 0,
  })
}

export function usePipelinePreview(
  providerType: string,
  config: ProviderConfig,
  organization?: string,
  search?: string,
  enableQuery = true
) {
  return useInfiniteQuery({
    queryKey: [
      'pipelinePreview',
      providerType,
      config.config,
      organization,
      search,
    ],
    queryFn: ({ pageParam = 1 }) =>
      service.previewProviderPipelines(
        providerType,
        config.config || {},
        organization,
        search,
        pageParam,
        PAGE_SIZES.PIPELINE_PREVIEW
      ),
    getNextPageParam: (lastPage: PaginatedAvailablePipelines) =>
      lastPage.has_more ? (lastPage.page ?? 0) + 1 : undefined,
    initialPageParam: 1,
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
    enabled: enableQuery && !!providerType && !!config.token && Object.keys(config.config || {}).length > 0 && !!organization,
  })
}

