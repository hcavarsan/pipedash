import { invoke } from '@tauri-apps/api/core'

import type { GlobalMetricsConfig, MetricsConfig, MetricsDashboard, MetricsStats } from '../../types/metrics'
import type { MetricsService } from '../types'

export const metricsTauriService: MetricsService = {
  getGlobalMetricsConfig: async () => invoke<GlobalMetricsConfig>('get_global_metrics_config'),

  updateGlobalMetricsConfig: async (enabled, defaultRetentionDays) =>
    invoke<void>('update_global_metrics_config', { enabled, defaultRetentionDays }),

  getPipelineMetricsConfig: async (pipelineId) => invoke<MetricsConfig>('get_pipeline_metrics_config', { pipelineId }),

  updatePipelineMetricsConfig: async (pipelineId, enabled, retentionDays) =>
    invoke<void>('update_pipeline_metrics_config', { pipelineId, enabled, retentionDays }),

  getMetricsDashboard: async (query) => invoke<MetricsDashboard>('get_metrics_dashboard', { query }),

  getMetricsStorageStats: async () => invoke<MetricsStats>('get_metrics_storage_stats'),

  flushPipelineMetrics: async (pipelineId) =>
    invoke<number>('flush_pipeline_metrics', { pipelineId: pipelineId ?? null }),
}
