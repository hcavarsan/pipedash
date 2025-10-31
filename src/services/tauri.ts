import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

import type {
  AvailablePipeline,
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
    token: string,
    config: Record<string, string>
  ): Promise<AvailablePipeline[]> => {
    return invoke<AvailablePipeline[]>('preview_provider_pipelines', {
      providerType,
      token,
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
}
