import {
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { REFRESH_INTERVALS } from '../../../constants/intervals'
import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { RunLogDocument } from '../../../types/logs'
import { displayErrorNotification, displaySuccessNotification, errorMessage } from '../../../utils/errorDisplay'
import { logLineCache } from '../../logs/lib/lineCache'

interface CacheStats {
  pipelines_count: number
  run_history_count: number
  workflow_params_count: number
  metrics_count: number
  run_logs_bytes: number
}

export function useCacheStats(): UseQueryResult<CacheStats, Error> {
  return useQuery({
    queryKey: queryKeys.cache.stats(),
    queryFn: () => service.getCacheStats(),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    refetchInterval: REFRESH_INTERVALS.CACHE_STATS,
    refetchIntervalInBackground: false,
  })
}

async function resetRunLogs(queryClient: QueryClient) {
  await queryClient.cancelQueries({ queryKey: queryKeys.runLogs.all })
  queryClient.setQueriesData({ queryKey: ['runLogs', 'doc'] }, {
    state: 'loading',
    received_bytes: 0,
  } satisfies RunLogDocument)
  queryClient.removeQueries({ queryKey: ['runLogs', 'search'] })
  logLineCache.clear()
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['runLogs', 'tree'] }),
    queryClient.invalidateQueries({ queryKey: ['runLogs', 'doc'] }),
    queryClient.invalidateQueries({ queryKey: queryKeys.cache.all }),
  ])
}

export function useClearCache(): UseMutationResult<void, Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearAllCaches(),

    onSuccess: async () => {
      await resetRunLogs(queryClient)
      void queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.runs.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.workflows.all })

      displaySuccessNotification('All caches cleared successfully', 'Cache cleared')
    },

    onError: (error: Error) => {
      displayErrorNotification(errorMessage(error, 'Try clearing the caches again.'), "Couldn't clear caches")
    },
  })
}

export function useClearPipelinesCache(): UseMutationResult<number, Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearPipelinesCache(),

    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })

      displaySuccessNotification('Pipeline cache cleared successfully', 'Pipelines cache cleared')
    },

    onError: (error: Error) => {
      displayErrorNotification(
        errorMessage(error, 'Try clearing the pipelines cache again.'),
        "Couldn't clear pipelines cache"
      )
    },
  })
}

export function useClearRunHistoryCache(): UseMutationResult<void, Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearAllRunHistoryCaches(),

    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.runs.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })

      displaySuccessNotification('Run history cache cleared successfully', 'Run history cache cleared')
    },

    onError: (error: Error) => {
      displayErrorNotification(
        errorMessage(error, 'Try clearing the run history cache again.'),
        "Couldn't clear run history cache"
      )
    },
  })
}

export function useClearWorkflowParamsCache(): UseMutationResult<void, Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearWorkflowParamsCache(),

    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.workflows.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })

      displaySuccessNotification('Workflow parameters cache cleared successfully', 'Workflow parameters cache cleared')
    },

    onError: (error: Error) => {
      displayErrorNotification(
        errorMessage(error, 'Try clearing the workflow parameters cache again.'),
        "Couldn't clear workflow parameters cache"
      )
    },
  })
}

export function useClearRunLogsCache(): UseMutationResult<void, Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.clearRunLogsCache(),
    retry: false,

    onSuccess: async () => {
      await resetRunLogs(queryClient)

      displaySuccessNotification('Run logs cache cleared successfully', 'Run logs cache cleared')
    },

    onError: (error: Error) => {
      displayErrorNotification(
        errorMessage(error, 'Try clearing the run logs cache again.'),
        "Couldn't clear run logs cache"
      )
    },
  })
}
