import { notifications } from '@mantine/notifications'
import {
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
  AggregatedMetrics,
  AggregationPeriod,
  AggregationType,
  GlobalMetricsConfig,
  MetricsConfig,
  MetricsStats,
  MetricType,
} from '../../../types/domain'
import { errorMessage } from '../../../utils/errorDisplay'

const advancePeriod = (date: Date, period: AggregationPeriod): Date => {
  const next = new Date(date)

  switch (period) {
    case 'hourly':
      next.setUTCHours(next.getUTCHours() + 1)
      break
    case 'daily':
      next.setUTCDate(next.getUTCDate() + 1)
      break
    case 'weekly':
      next.setUTCDate(next.getUTCDate() + 7)
      break
    case 'monthly':
      next.setUTCMonth(next.getUTCMonth() + 1)
      break
  }

  return next
}

const fillMissingPeriods = (data: AggregatedMetrics): AggregatedMetrics => {
  const filled: AggregatedMetrics['metrics'] = []

  data.metrics.forEach((metric, index) => {
    filled.push(metric)

    const following = data.metrics[index + 1]

    if (!following) {
      return
    }

    const end = new Date(following.timestamp)

    for (
      let gap = advancePeriod(new Date(metric.timestamp), data.aggregation_period);
      gap < end;
      gap = advancePeriod(gap, data.aggregation_period)
    ) {
      filled.push({ timestamp: gap.toISOString(), value: 0, count: 0, min: null, max: null, avg: 0 })
    }
  })

  return { ...data, metrics: filled, total_count: filled.length }
}

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

export function useAggregatedMetrics(params: {
  metricType: MetricType
  aggregationPeriod: AggregationPeriod
  aggregationType?: AggregationType | undefined
  pipelineId?: string | undefined
  startDate?: string | undefined
  endDate?: string | undefined
  limit?: number | undefined
  enabled?: boolean | undefined
}): UseQueryResult<AggregatedMetrics, Error> {
  const { enabled = true, ...queryParams } = params

  return useQuery({
    queryKey: [
      ...queryKeys.metrics.aggregated({
        metricType: queryParams.metricType,
        aggregationPeriod: queryParams.aggregationPeriod,
        pipelineId: queryParams.pipelineId,
      }),
      queryParams.aggregationType,
      queryParams.startDate,
      queryParams.endDate,
      queryParams.limit,
    ],
    queryFn: () =>
      service.queryAggregatedMetrics(
        queryParams.metricType,
        queryParams.aggregationPeriod,
        queryParams.aggregationType,
        queryParams.pipelineId,
        queryParams.startDate,
        queryParams.endDate,
        queryParams.limit
      ),
    select: fillMissingPeriods,
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
      void queryClient.invalidateQueries({ queryKey: queryKeys.metrics.globalConfig() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.metrics.stats() })

      notifications.show({
        title: 'Global metrics settings saved',
        message: 'Metrics collection and retention settings were updated.',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't save global metrics settings",
        message: errorMessage(error, 'Try saving the global metrics settings again.'),
        color: 'red',
      })
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

      notifications.show({
        title: 'Pipeline metrics settings saved',
        message: 'Metrics collection and retention settings were updated.',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't save pipeline metrics settings",
        message: errorMessage(error, 'Try saving the pipeline metrics settings again.'),
        color: 'red',
      })
    },
  })
}

export function useFlushMetrics(): UseMutationResult<number, Error, string | undefined, unknown> {
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

      notifications.show({
        title: 'Metrics deleted',
        message: `Deleted ${deletedCount} metric${deletedCount !== 1 ? 's' : ''}`,
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't delete metrics",
        message: errorMessage(error, 'Try deleting the stored metrics again.'),
        color: 'red',
      })
    },
  })
}
