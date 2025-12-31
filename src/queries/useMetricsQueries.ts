import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type {
  AggregationPeriod,
  AggregationType,
  MetricType,
} from '../types'


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
    queryKey: queryKeys.metrics.aggregated({
      metricType: queryParams.metricType,
      aggregationPeriod: queryParams.aggregationPeriod,
      pipelineId: queryParams.pipelineId,
    }),
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
        title: 'Configuration Saved',
        message: 'Global metrics configuration updated successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Update Configuration',
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
        title: 'Configuration Saved',
        message: 'Pipeline metrics configuration updated successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Update Configuration',
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
        title: 'Metrics Flushed',
        message: `Deleted ${deletedCount} metric${deletedCount !== 1 ? 's' : ''}`,
        color: 'blue',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Flush Metrics',
        message: error.message || 'Unknown error occurred',
        color: 'red',
      })
    },
  })
}
