import type { GlobalMetricsConfig, MetricsConfig, MetricsDashboard, MetricsStats } from '../../types/metrics'
import type { HttpClient } from '../http'
import type { MetricsService } from '../types'

export function createMetricsApi(client: HttpClient): MetricsService {
  return {
    getGlobalMetricsConfig: async () => client.get<GlobalMetricsConfig>('/metrics/config'),

    updateGlobalMetricsConfig: async (enabled, defaultRetentionDays) => {
      await client.requestVoid('PUT', '/metrics/config', {
        enabled,
        default_retention_days: defaultRetentionDays,
      })
    },

    getPipelineMetricsConfig: async (pipelineId) =>
      client.get<MetricsConfig>(`/metrics/pipelines/${encodeURIComponent(pipelineId)}/config`),

    updatePipelineMetricsConfig: async (pipelineId, enabled, retentionDays) => {
      await client.requestVoid('PUT', `/metrics/pipelines/${encodeURIComponent(pipelineId)}/config`, {
        enabled,
        retention_days: retentionDays,
      })
    },

    getMetricsDashboard: async (query) => client.post<MetricsDashboard>('/metrics/dashboard', query),

    getMetricsStorageStats: async () => client.get<MetricsStats>('/metrics/storage/stats'),

    flushPipelineMetrics: async (pipelineId) => {
      const result = await client.post<{ flushed: number }>('/metrics/flush', {
        pipeline_id: pipelineId ?? null,
      })

      return result.flushed
    },
  }
}
