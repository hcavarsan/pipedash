import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { REFRESH_INTERVALS } from '../constants/intervals'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import { errorMessage } from '../utils/errorDisplay'

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
        title: 'Cache cleared',
        message: 'All caches cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't clear caches",
        message: errorMessage(error, 'Try clearing the caches again.'),
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
        title: 'Pipelines cache cleared',
        message: 'Pipeline cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't clear pipelines cache",
        message: errorMessage(error, 'Try clearing the pipelines cache again.'),
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
        title: 'Run history cache cleared',
        message: 'Run history cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't clear run history cache",
        message: errorMessage(error, 'Try clearing the run history cache again.'),
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
        title: 'Workflow parameters cache cleared',
        message: 'Workflow parameters cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't clear workflow parameters cache",
        message: errorMessage(error, 'Try clearing the workflow parameters cache again.'),
        color: 'red',
      })
    },
  })
}
