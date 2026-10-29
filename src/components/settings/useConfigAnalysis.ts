import { type UseQueryResult, useQuery, useQueryClient } from '@tanstack/react-query'

import { DEBOUNCE_DELAYS } from '../../constants/intervals'
import { useDebounce } from '../../hooks/useDebounce'
import { GC_TIMES, STALE_TIMES } from '../../lib/cacheConfig'
import { logger } from '../../lib/logger'
import { queryKeys } from '../../lib/queryKeys'
import { service } from '../../services'
import type { ConfigAnalysisResponse } from '../../types'

async function analyzeConfigContent(content: string): Promise<ConfigAnalysisResponse> {
  try {
    return await service.analyzeConfig(content)
  } catch (error) {
    logger.error('ConfigEditorModal', 'Analysis failed', error)

    throw error
  }
}

export function useConfigAnalysis(content: string, enabled: boolean): UseQueryResult<ConfigAnalysisResponse, Error> {
  const settledContent = useDebounce(content, DEBOUNCE_DELAYS.AUTO_SAVE)

  return useQuery({
    queryKey: queryKeys.storage.analysis(content),
    queryFn: () => analyzeConfigContent(content),
    enabled: enabled && settledContent === content,
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    retry: false,
    refetchOnWindowFocus: false,
  })
}

export function useAnalyzeConfigNow(): (content: string) => Promise<ConfigAnalysisResponse> {
  const queryClient = useQueryClient()

  return (content) =>
    queryClient.fetchQuery({
      queryKey: queryKeys.storage.analysis(content),
      queryFn: () => analyzeConfigContent(content),
      staleTime: 0,
      gcTime: GC_TIMES.SHORT,
    })
}
