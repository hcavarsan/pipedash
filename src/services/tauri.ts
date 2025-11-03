import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

import type {
  AggregatedMetrics,
  AggregationPeriod,
  AggregationType,
  AvailablePipeline,
  GlobalMetricsConfig,
  MetricEntry,
  MetricsConfig,
  MetricsStats,
  MetricType,
  PaginatedRunHistory,
  Pipeline,
  PipelineRun,
  PluginMetadata,
  ProviderConfig,
  ProviderSummary,
  TriggerParams,
  WorkflowParameter,
} from '../types'

export const tauriService = {
  addProvider: async (config: ProviderConfig): Promise<number> => {
    return invoke<number>('add_provider', { config })
  },

  listProviders: async (): Promise<ProviderSummary[]> => {
    return invoke<ProviderSummary[]>('list_providers')
  },

  getProvider: async (id: number): Promise<ProviderConfig & { id: number }> => {
    const config = await invoke<ProviderConfig>('get_provider', { id })


    
return { ...config, id }
  },

  updateProvider: async (id: number, config: ProviderConfig): Promise<void> => {
    return invoke<void>('update_provider', { id, config })
  },

  updateProviderRefreshInterval: async (id: number, refreshInterval: number): Promise<void> => {
    return invoke<void>('update_provider_refresh_interval', { id, refreshInterval })
  },

  removeProvider: async (id: number): Promise<void> => {
    return invoke<void>('remove_provider', { id })
  },

  previewProviderPipelines: async (
    providerType: string,
    config: Record<string, string>
  ): Promise<AvailablePipeline[]> => {
    return invoke<AvailablePipeline[]>('preview_provider_pipelines', {
      providerType,
      config,
    })
  },

  getProviderFieldOptions: async (
    providerType: string,
    fieldKey: string,
    config: Record<string, string>
  ): Promise<string[]> => {
    return invoke<string[]>('get_provider_field_options', {
      providerType,
      fieldKey,
      config,
    })
  },

  fetchPipelines: async (providerId?: number): Promise<Pipeline[]> => {
    return invoke<Pipeline[]>('fetch_pipelines', {
      providerId: providerId ?? null,
    })
  },

  getCachedPipelines: async (providerId?: number): Promise<Pipeline[]> => {
    return invoke<Pipeline[]>('get_cached_pipelines', {
      providerId: providerId ?? null,
    })
  },

  fetchRunHistory: async (
    pipelineId: string,
    page?: number,
    pageSize?: number
  ): Promise<PaginatedRunHistory> => {
    return invoke<PaginatedRunHistory>('fetch_run_history', {
      pipelineId,
      page: page ?? null,
      pageSize: pageSize ?? null,
    })
  },

  triggerPipeline: async (params: TriggerParams): Promise<string> => {
    return invoke<string>('trigger_pipeline', { params })
  },

  getWorkflowParameters: async (workflowId: string): Promise<WorkflowParameter[]> => {
    return invoke<WorkflowParameter[]>('get_workflow_parameters', { workflowId })
  },

  refreshAll: async (): Promise<void> => {
    return invoke<void>('refresh_all')
  },

  setRefreshMode: async (mode: 'active' | 'idle'): Promise<void> => {
    return invoke<void>('set_refresh_mode', { mode })
  },

  getRefreshMode: async (): Promise<'active' | 'idle'> => {
    return invoke<'active' | 'idle'>('get_refresh_mode')
  },

  getWorkflowRunDetails: async (
    pipelineId: string,
    runNumber: number
  ): Promise<PipelineRun> => {
    return invoke<PipelineRun>('get_workflow_run_details', {
      pipelineId,
      runNumber,
    })
  },

  cancelPipelineRun: async (
    pipelineId: string,
    runNumber: number
  ): Promise<void> => {
    return invoke<void>('cancel_pipeline_run', {
      pipelineId,
      runNumber,
    })
  },

  openUrl: async (url: string): Promise<void> => {
    await openUrl(url)
  },

  getAvailablePlugins: async (): Promise<PluginMetadata[]> => {
    return invoke<PluginMetadata[]>('get_available_plugins')
  },

  clearRunHistoryCache: async (pipelineId: string): Promise<void> => {
    return invoke<void>('clear_run_history_cache', { pipelineId })
  },

  getGlobalMetricsConfig: async (): Promise<GlobalMetricsConfig> => {
    return invoke<GlobalMetricsConfig>('get_global_metrics_config')
  },

  updateGlobalMetricsConfig: async (
    enabled: boolean,
    defaultRetentionDays: number
  ): Promise<void> => {
    return invoke<void>('update_global_metrics_config', {
      enabled,
      defaultRetentionDays,
    })
  },

  getPipelineMetricsConfig: async (pipelineId: string): Promise<MetricsConfig> => {
    return invoke<MetricsConfig>('get_pipeline_metrics_config', { pipelineId })
  },

  updatePipelineMetricsConfig: async (
    pipelineId: string,
    enabled: boolean,
    retentionDays: number
  ): Promise<void> => {
    return invoke<void>('update_pipeline_metrics_config', {
      pipelineId,
      enabled,
      retentionDays,
    })
  },

  queryPipelineMetrics: async (
    pipelineId?: string,
    metricType?: MetricType,
    startDate?: string,
    endDate?: string,
    limit?: number
  ): Promise<MetricEntry[]> => {
    return invoke<MetricEntry[]>('query_pipeline_metrics', {
      pipelineId: pipelineId ?? null,
      metricType: metricType ?? null,
      startDate: startDate ?? null,
      endDate: endDate ?? null,
      limit: limit ?? null,
    })
  },

  queryAggregatedMetrics: async (
    metricType: MetricType,
    aggregationPeriod: AggregationPeriod,
    aggregationType?: AggregationType,
    pipelineId?: string,
    startDate?: string,
    endDate?: string,
    limit?: number
  ): Promise<AggregatedMetrics> => {
    return invoke<AggregatedMetrics>('query_aggregated_metrics', {
      pipelineId: pipelineId ?? null,
      metricType,
      aggregationPeriod,
      aggregationType: aggregationType ?? null,
      startDate: startDate ?? null,
      endDate: endDate ?? null,
      limit: limit ?? null,
    })
  },

  getMetricsStorageStats: async (): Promise<MetricsStats> => {
    return invoke<MetricsStats>('get_metrics_storage_stats')
  },

  flushPipelineMetrics: async (pipelineId?: string): Promise<number> => {
    return invoke<number>('flush_pipeline_metrics', {
      pipelineId: pipelineId ?? null,
    })
  },

  getCacheStats: async (): Promise<{
    pipelines_count: number
    run_history_count: number
    workflow_params_count: number
    metrics_count: number
  }> => {
    return invoke('get_cache_stats')
  },

  clearPipelinesCache: async (): Promise<number> => {
    return invoke<number>('clear_pipelines_cache')
  },

  clearAllRunHistoryCaches: async (): Promise<void> => {
    return invoke<void>('clear_all_run_history_caches')
  },

  clearWorkflowParamsCache: async (): Promise<void> => {
    return invoke<void>('clear_workflow_params_cache')
  },

  clearAllCaches: async (): Promise<void> => {
    return invoke<void>('clear_all_caches')
  },
}
