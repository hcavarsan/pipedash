import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { REFRESH_INTERVALS } from '../constants/intervals'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'

export function useCacheStats() {
  return useQuery({
    queryKey: queryKeys.cache.stats(),
    queryFn: () => service.getCacheStats(),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    refetchInterval: REFRESH_INTERVALS.CACHE_STATS,
    refetchIntervalInBackground: false,
  })
}

export function useClearCache() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearAllCaches(),

    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      notifications.show({
        title: 'Cache Cleared',
        message: 'All caches cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Clear Cache',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function useClearPipelinesCache() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearPipelinesCache(),

    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })

      notifications.show({
        title: 'Pipelines Cache Cleared',
        message: 'Pipeline cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Clear Pipelines Cache',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function useClearRunHistoryCache() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearAllRunHistoryCaches(),

    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.runs.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })

      notifications.show({
        title: 'Run History Cache Cleared',
        message: 'Run history cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Clear Run History Cache',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function useClearWorkflowParamsCache() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearWorkflowParamsCache(),

    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.workflows.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })

      notifications.show({
        title: 'Workflow Parameters Cache Cleared',
        message: 'Workflow parameters cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Clear Workflow Parameters Cache',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}
