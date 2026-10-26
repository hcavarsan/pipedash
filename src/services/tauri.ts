import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { platform } from '@tauri-apps/plugin-os'
import { REFRESH_TIMEOUT_MS } from '../constants/timeouts'
import { logger } from '../lib/logger'
import {
  type AggregatedMetrics,
  type AggregationPeriod,
  type AggregationType,
  type ConfigAnalysisResponse,
  type ConfigContentResponse,
  createError,
  type FeatureAvailability,
  type GeneralConfig,
  type GlobalMetricsConfig,
  type MetricEntry,
  type MetricsConfig,
  type MetricsStats,
  type MetricType,
  type MigrationOptions,
  type MigrationPlan,
  type MigrationResult,
  type NotificationTarget,
  type Organization,
  type PaginatedAvailablePipelines,
  type PaginatedRunHistory,
  type PermissionCheckResult,
  type PermissionStatus,
  type PipedashConfig,
  type Pipeline,
  type PipelineRun,
  type PluginMetadata,
  type ProviderConfig,
  type ProviderImportResult,
  type ProviderSummary,
  type RefreshResult,
  type ReviewDecision,
  type SetupStatus,
  type StorageConfigResponse,
  type StoragePathsResponse,
  type TableSchema,
  type TriggerParams,
  toPipedashError,
  type UnlockVaultResponse,
  type ValidationResult,
  type VaultStatusResponse,
  type WorkflowParameter,
} from '../types'
import { DEFAULT_RETRY_CONFIG, shouldRetry, withRetry } from '../utils/retryLogic'

const TAURI_COMMAND_TIMEOUTS: Record<string, number> = {
  factory_reset: 120000,
  list_pipelines: 90000,
  get_cached_pipelines: 90000,
  preview_provider_pipelines: 60000,
  fetch_provider_organizations: 60000,
  add_provider: 120000,
  default: 45000,
}

async function invokeWithTimeout<T>(
  cmd: string,
  args?: Record<string, unknown>,
  customTimeout?: number,
  enableRetry = true
): Promise<T> {
  const timeout = customTimeout ?? TAURI_COMMAND_TIMEOUTS[cmd] ?? TAURI_COMMAND_TIMEOUTS.default

  const executeCommand = async () => {
    try {
      return await Promise.race([
        invoke<T>(cmd, args),
        new Promise<T>((_, reject) =>
          setTimeout(() => reject(createError('timeout', `Command '${cmd}' timeout`, { timeoutMs: timeout })), timeout)
        ),
      ])
    } catch (error) {
      throw toPipedashError(error)
    }
  }

  if (enableRetry) {
    return withRetry(executeCommand, {
      ...DEFAULT_RETRY_CONFIG,
      shouldRetry: (error: Error) => {
        return shouldRetry(error, 1, DEFAULT_RETRY_CONFIG)
      },
    })
  }

  return executeCommand()
}

export const tauriService = {
  addProvider: async (config: ProviderConfig): Promise<number> => {
    return invokeWithTimeout<number>('add_provider', { config })
  },

  listProviders: async (): Promise<ProviderSummary[]> => {
    logger.debug('tauriService', 'Invoking list_providers...')

    return withRetry(async () => {
      try {
        const result = await invoke<ProviderSummary[]>('list_providers')

        logger.debug('tauriService', 'list_providers returned', { count: result?.length ?? 0, unit: 'items' })

        return result
      } catch (err) {
        logger.error('tauriService', 'list_providers error', err)
        throw toPipedashError(err)
      }
    }, DEFAULT_RETRY_CONFIG)
  },

  getProvider: async (id: number): Promise<ProviderConfig & { id: number }> => {
    try {
      const config = await invoke<ProviderConfig>('get_provider', { id })

      return { ...config, id }
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  updateProvider: async (id: number, config: ProviderConfig): Promise<void> => {
    try {
      return await invoke<void>('update_provider', { id, config })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  updateProviderRefreshInterval: async (id: number, refreshInterval: number): Promise<void> => {
    try {
      return await invoke<void>('update_provider_refresh_interval', { id, refreshInterval })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  removeProvider: async (id: number): Promise<void> => {
    try {
      return await invoke<void>('remove_provider', { id })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  exportProviders: async (): Promise<string> => {
    try {
      return await invoke<string>('export_providers')
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  importProviders: async (content: string): Promise<ProviderImportResult> => {
    try {
      return await invoke<ProviderImportResult>('import_providers', { content })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  fetchProviderOrganizations: async (providerType: string, config: Record<string, string>): Promise<Organization[]> => {
    return invokeWithTimeout<Organization[]>('fetch_provider_organizations', {
      providerType,
      config,
    })
  },

  previewProviderPipelines: async (
    providerType: string,
    config: Record<string, string>,
    org?: string,
    search?: string,
    page?: number,
    pageSize?: number
  ): Promise<PaginatedAvailablePipelines> => {
    return invokeWithTimeout<PaginatedAvailablePipelines>('preview_provider_pipelines', {
      providerType,
      config,
      org: org ?? null,
      search: search ?? null,
      page,
      pageSize,
    })
  },

  getProviderFieldOptions: async (
    providerType: string,
    fieldKey: string,
    config: Record<string, string>
  ): Promise<string[]> => {
    try {
      return await invoke<string[]>('get_provider_field_options', {
        providerType,
        fieldKey,
        config,
      })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  getProviderPermissions: async (providerId: number): Promise<PermissionStatus | null> => {
    try {
      return await invoke<PermissionStatus | null>('get_provider_permissions', { providerId })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  getProviderFeatures: async (providerId: number): Promise<FeatureAvailability[]> => {
    try {
      return await invoke<FeatureAvailability[]>('get_provider_features', { providerId })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  getProviderTableSchema: async (providerId: number): Promise<TableSchema> => {
    try {
      return await invoke<TableSchema>('get_provider_table_schema', { providerId })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  validateProviderCredentials: async (
    providerType: string,
    config: Record<string, string>
  ): Promise<ValidationResult> => {
    try {
      return await invoke<ValidationResult>('validate_provider_credentials', {
        providerType,
        config,
      })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  checkProviderPermissions: async (
    providerType: string,
    config: Record<string, string>
  ): Promise<PermissionCheckResult> => {
    try {
      return await invoke<PermissionCheckResult>('check_provider_permissions', {
        providerType,
        config,
      })
    } catch (error) {
      throw toPipedashError(error)
    }
  },

  getCachedPipelines: async (providerId?: number): Promise<Pipeline[]> => {
    logger.debug('tauriService', 'Invoking get_cached_pipelines...', { providerId })

    return withRetry(async () => {
      try {
        const result = await invokeWithTimeout<Pipeline[]>('get_cached_pipelines', {
          providerId: providerId ?? null,
        })

        logger.debug('tauriService', 'get_cached_pipelines returned', { count: result?.length ?? 0, unit: 'items' })

        return result
      } catch (err) {
        logger.error('tauriService', 'get_cached_pipelines error', err)
        throw err
      }
    }, DEFAULT_RETRY_CONFIG)
  },

  fetchRunHistory: async (pipelineId: string, page?: number, pageSize?: number): Promise<PaginatedRunHistory> => {
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

  refreshProvider: async (id: number): Promise<RefreshResult> => {
    return invokeWithTimeout<RefreshResult>('refresh_provider', { id }, REFRESH_TIMEOUT_MS, false)
  },

  refreshAll: async (): Promise<RefreshResult[]> => {
    return invokeWithTimeout<RefreshResult[]>('refresh_all', undefined, REFRESH_TIMEOUT_MS, false)
  },

  updateGeneralSettings: async (update: GeneralConfig): Promise<void> => {
    return invokeWithTimeout<void>('update_general_settings', { update })
  },

  setRefreshMode: async (mode: 'active' | 'idle'): Promise<void> => {
    return invoke<void>('set_refresh_mode', { mode })
  },

  showSystemNotification: async (title: string, body: string, target: NotificationTarget): Promise<void> => {
    return invoke<void>('show_system_notification', { title, body, target })
  },

  getWorkflowRunDetails: async (pipelineId: string, runNumber: number): Promise<PipelineRun> => {
    return invoke<PipelineRun>('get_workflow_run_details', {
      pipelineId,
      runNumber,
    })
  },

  cancelPipelineRun: async (pipelineId: string, runNumber: number): Promise<void> => {
    return invoke<void>('cancel_pipeline_run', {
      pipelineId,
      runNumber,
    })
  },

  reviewPipelineRun: async (pipelineId: string, runNumber: number, decision: ReviewDecision): Promise<void> => {
    return invoke<void>('review_pipeline_run', {
      pipelineId,
      runNumber,
      decision,
    })
  },

  rerunFailedJobs: async (pipelineId: string, runNumber: number): Promise<void> => {
    return invoke<void>('rerun_failed_jobs', {
      pipelineId,
      runNumber,
    })
  },

  rerunRun: async (pipelineId: string, runNumber: number): Promise<void> => {
    return invoke<void>('rerun_pipeline_run', {
      pipelineId,
      runNumber,
    })
  },

  openUrl: async (url: string): Promise<void> => {
    await openUrl(url, ['ios', 'android'].includes(platform()) ? 'inAppBrowser' : undefined)
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

  updateGlobalMetricsConfig: async (enabled: boolean, defaultRetentionDays: number): Promise<void> => {
    return invoke<void>('update_global_metrics_config', {
      enabled,
      defaultRetentionDays,
    })
  },

  getPipelineMetricsConfig: async (pipelineId: string): Promise<MetricsConfig> => {
    return invoke<MetricsConfig>('get_pipeline_metrics_config', { pipelineId })
  },

  updatePipelineMetricsConfig: async (pipelineId: string, enabled: boolean, retentionDays: number): Promise<void> => {
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

  getTablePreferences: async (providerId: number, tableId: string): Promise<string | null> => {
    return invoke<string | null>('get_table_preferences', { providerId, tableId })
  },

  saveTablePreferences: async (providerId: number, tableId: string, preferencesJson: string): Promise<void> => {
    return invoke<void>('save_table_preferences', {
      providerId,
      tableId,
      preferencesJson,
    })
  },

  getDefaultTablePreferences: async (providerId: number, tableId: string): Promise<string> => {
    return invoke<string>('get_default_table_preferences', { providerId, tableId })
  },

  checkSetupStatus: async (): Promise<SetupStatus> => {
    return invoke<SetupStatus>('check_setup_status')
  },

  createInitialConfig: async (config: PipedashConfig, vaultPassword?: string): Promise<void> => {
    return invoke<void>('create_initial_config', { config, vaultPassword })
  },

  bootstrapApp: async (): Promise<void> => {
    return invoke<void>('bootstrap_app')
  },

  getStorageConfig: async (): Promise<StorageConfigResponse> => {
    return invoke<StorageConfigResponse>('get_storage_config')
  },

  getVaultPasswordStatus: async (): Promise<{ is_set: boolean; env_var_name: string }> => {
    return invoke<{ is_set: boolean; env_var_name: string }>('get_vault_password_status')
  },

  getVaultStatus: async (): Promise<VaultStatusResponse> => {
    return invoke('get_vault_status')
  },

  unlockVault: async (password: string): Promise<UnlockVaultResponse> => {
    return invoke('unlock_vault', { password })
  },

  lockVault: async (): Promise<UnlockVaultResponse> => {
    return invoke('lock_vault')
  },

  getConfigContent: async (): Promise<ConfigContentResponse> => {
    return invoke<ConfigContentResponse>('get_config_content')
  },

  saveConfigContent: async (content: string): Promise<void> => {
    return invoke<void>('save_config_content', { content })
  },

  analyzeConfig: async (content: string): Promise<ConfigAnalysisResponse> => {
    return invoke<ConfigAnalysisResponse>('analyze_config', { content })
  },

  getStoragePaths: async (): Promise<StoragePathsResponse> => {
    return invoke<StoragePathsResponse>('get_storage_paths')
  },

  getDefaultDataDir: async (): Promise<string> => {
    return invoke<string>('get_default_data_dir')
  },

  getEffectiveDataDir: async (config: PipedashConfig): Promise<string> => {
    return invoke<string>('get_effective_data_dir', { config })
  },

  checkDatabaseExists: async (config: PipedashConfig): Promise<{ exists: boolean; path: string }> => {
    return invoke<{ exists: boolean; path: string }>('check_database_exists', { config })
  },

  saveStorageConfig: async (config: PipedashConfig, tokenPassword?: string): Promise<void> => {
    return invoke<void>('save_storage_config', { config, tokenPassword })
  },

  testStorageConnection: async (
    config: PipedashConfig
  ): Promise<{
    success: boolean
    message: string
  }> => {
    return invoke<{ success: boolean; message: string }>('test_storage_connection', { config })
  },

  planStorageMigration: async (targetConfig: PipedashConfig, options?: MigrationOptions): Promise<MigrationPlan> => {
    return invoke<MigrationPlan>('plan_storage_migration', {
      targetConfig,
      options: options ?? {},
    })
  },

  executeMigration: async (plan: MigrationPlan, options?: MigrationOptions): Promise<MigrationResult> => {
    return invoke<MigrationResult>('execute_storage_migration', {
      plan,
      options: options ?? {},
    })
  },

  factoryReset: async (): Promise<{
    providers_removed: number
    caches_cleared: boolean
    tokens_cleared: boolean
    metrics_cleared: boolean
  }> => {
    return invokeWithTimeout('factory_reset')
  },

  restartApp: async (): Promise<void> => {
    return invoke<void>('restart_app')
  },
}
