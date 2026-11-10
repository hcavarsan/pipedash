import {
  keepPreviousData,
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useCallback } from 'react'
import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type {
  AggregationPeriod,
  AggregationType,
  GlobalMetricsConfig,
  MetricsConfig,
  MetricsDashboard,
  MetricsStats,
  MetricTableSort,
  MetricType,
} from '../../../types/metrics'
import type { DateRange } from '../../../utils/dateRange'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'

export function useInvalidatePipelineMetrics(pipelineId: string): () => Promise<void> {
  const queryClient = useQueryClient()

  return useCallback(
    () => queryClient.invalidateQueries({ queryKey: [...queryKeys.metrics.all, 'aggregated', { pipelineId }] }),
    [queryClient, pipelineId]
  )
}

export function useGlobalMetricsConfig(): UseQueryResult<GlobalMetricsConfig, Error> {
  return useQuery({
    queryKey: queryKeys.metrics.globalConfig(),
    queryFn: () => service.getGlobalMetricsConfig(),
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.SHORT,
  })
}

export function usePipelineMetricsConfig(pipelineId: string): UseQueryResult<MetricsConfig, Error> {
  return useQuery({
    queryKey: queryKeys.metrics.pipelineConfig(pipelineId),
    queryFn: () => service.getPipelineMetricsConfig(pipelineId),
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: !!pipelineId,
  })
}

export function useMetricsDashboard(params: {
  metricType: MetricType
  aggregationPeriod: AggregationPeriod
  aggregationType?: AggregationType | undefined
  pipelineId?: string | undefined
  dateRange: DateRange
  limit?: number | undefined
  tableSort?: MetricTableSort | undefined
  enabled?: boolean | undefined
}): UseQueryResult<MetricsDashboard, Error> {
  const { enabled = true, ...queryParams } = params

  return useQuery({
    queryKey: [
      ...queryKeys.metrics.aggregated({
        metricType: queryParams.metricType,
        aggregationPeriod: queryParams.aggregationPeriod,
        pipelineId: queryParams.pipelineId,
      }),
      queryParams.aggregationType,
      queryParams.dateRange,
      queryParams.limit,
      queryParams.tableSort,
    ],
    queryFn: () =>
      service.getMetricsDashboard({
        pipeline_id: queryParams.pipelineId,
        metric_type: queryParams.metricType,
        aggregation_period: queryParams.aggregationPeriod,
        aggregation_type: queryParams.aggregationType,
        date_range: queryParams.dateRange,
        utc_offset_minutes: -new Date().getTimezoneOffset(),
        limit: queryParams.limit,
        table_sort: queryParams.tableSort,
      }),
    placeholderData: keepPreviousData,
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
    enabled,
  })
}

export function useMetricsStorageStats(): UseQueryResult<MetricsStats, Error> {
  return useQuery({
    queryKey: queryKeys.metrics.stats(),
    queryFn: () => service.getMetricsStorageStats(),
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
  })
}

export function useUpdateGlobalMetricsConfig(): UseMutationResult<
  void,
  Error,
  { enabled: boolean; retentionDays: number },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ enabled, retentionDays }: { enabled: boolean; retentionDays: number }) =>
      service.updateGlobalMetricsConfig(enabled, retentionDays),

    onSuccess: (_data, { enabled, retentionDays }) => {
      queryClient.setQueryData<GlobalMetricsConfig>(queryKeys.metrics.globalConfig(), (config) =>
        config ? { ...config, enabled, default_retention_days: retentionDays } : config
      )
      void queryClient.invalidateQueries({ queryKey: queryKeys.metrics.all })

      displaySuccessNotification(
        'Metrics collection and retention settings were updated.',
        'Global metrics settings saved'
      )
    },
  })
}

export function useUpdatePipelineMetricsConfig(): UseMutationResult<
  void,
  Error,
  { pipelineId: string; enabled: boolean; retentionDays: number },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      pipelineId,
      enabled,
      retentionDays,
    }: {
      pipelineId: string
      enabled: boolean
      retentionDays: number
    }) => service.updatePipelineMetricsConfig(pipelineId, enabled, retentionDays),

    onSuccess: (_data, { pipelineId }) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.metrics.pipelineConfig(pipelineId),
      })

      void queryClient.invalidateQueries({ queryKey: queryKeys.metrics.stats() })

      displaySuccessNotification(
        'Metrics collection and retention settings were updated.',
        'Pipeline metrics settings saved'
      )
    },
  })
}

export function useFlushMetrics({
  toastErrors = true,
}: {
  toastErrors?: boolean
} = {}): UseMutationResult<number, Error, string | undefined, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipelineId?: string) => service.flushPipelineMetrics(pipelineId),

    onSuccess: (deletedCount, pipelineId) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.metrics.stats() })

      if (pipelineId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.metrics.all,
          predicate: (query): boolean => {
            const key = query.queryKey

            return (
              key[0] === 'metrics' &&
              key[1] === 'aggregated' &&
              key[2] !== undefined &&
              typeof key[2] === 'object' &&
              key[2] !== null &&
              'pipelineId' in key[2] &&
              key[2].pipelineId === pipelineId
            )
          },
        })
      } else {
        void queryClient.invalidateQueries({ queryKey: queryKeys.metrics.all })
      }

      displaySuccessNotification(`Deleted ${deletedCount} metric${deletedCount !== 1 ? 's' : ''}`, 'Metrics deleted')
    },

    onError: (error: Error) => {
      if (toastErrors) {
        displayErrorNotification(error, "Couldn't delete stored metrics")
      }
    },
  })
}
