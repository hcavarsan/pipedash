import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useCallback } from 'react'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { AggregatedMetrics, AggregationPeriod, AggregationType, MetricType } from '../types'

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

export function useInvalidatePipelineMetrics(pipelineId: string) {
  const queryClient = useQueryClient()

  return useCallback(
    () => queryClient.invalidateQueries({ queryKey: [...queryKeys.metrics.all, 'aggregated', { pipelineId }] }),
    [queryClient, pipelineId]
  )
}

export function useGlobalMetricsConfig() {
  return useQuery({
    queryKey: queryKeys.metrics.globalConfig(),
    queryFn: () => service.getGlobalMetricsConfig(),
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.SHORT,
  })
}

export function usePipelineMetricsConfig(pipelineId: string) {
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
  aggregationType?: AggregationType
  pipelineId?: string
  startDate?: string
  endDate?: string
  limit?: number
  enabled?: boolean
}) {
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

export function useMetricsStorageStats() {
  return useQuery({
    queryKey: queryKeys.metrics.stats(),
    queryFn: () => service.getMetricsStorageStats(),
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
  })
}

export function useUpdateGlobalMetricsConfig() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ enabled, retentionDays }: { enabled: boolean; retentionDays: number }) =>
      service.updateGlobalMetricsConfig(enabled, retentionDays),

    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.metrics.globalConfig() })

      queryClient.invalidateQueries({ queryKey: queryKeys.metrics.stats() })

      notifications.show({
        title: 'Global metrics settings saved',
        message: 'Metrics collection and retention settings were updated.',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't save global metrics settings",
        message: error.message || 'Unknown error occurred',
        color: 'red',
      })
    },
  })
}

export function useUpdatePipelineMetricsConfig() {
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
      queryClient.invalidateQueries({
        queryKey: queryKeys.metrics.pipelineConfig(pipelineId),
      })

      queryClient.invalidateQueries({ queryKey: queryKeys.metrics.stats() })

      notifications.show({
        title: 'Pipeline metrics settings saved',
        message: 'Metrics collection and retention settings were updated.',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't save pipeline metrics settings",
        message: error.message || 'Unknown error occurred',
        color: 'red',
      })
    },
  })
}

export function useFlushMetrics() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipelineId?: string) => service.flushPipelineMetrics(pipelineId),

    onSuccess: (deletedCount, pipelineId) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.metrics.stats() })

      if (pipelineId) {
        queryClient.invalidateQueries({
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
        queryClient.invalidateQueries({ queryKey: queryKeys.metrics.all })
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
        message: error.message || 'Unknown error occurred',
        color: 'red',
      })
    },
  })
}
