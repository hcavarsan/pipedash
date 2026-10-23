import { API_TIMEOUTS, REFRESH_TIMEOUT_MS } from '../constants/timeouts'
import { withBasePath } from '../lib/basePath'
import { logger } from '../lib/logger'
import { getToken, useAuthStore } from '../stores/authStore'
import type {
  AggregatedMetrics,
  AggregationPeriod,
  AggregationType,
  ConfigAnalysisResponse,
  ConfigContentResponse,
  FeatureAvailability,
  GeneralConfig,
  GlobalMetricsConfig,
  MetricEntry,
  MetricsConfig,
  MetricsStats,
  MetricType,
  MigrationOptions,
  MigrationPlan,
  MigrationResult,
  Organization,
  PaginatedAvailablePipelines,
  PaginatedRunHistory,
  PermissionCheckResult,
  PermissionStatus,
  PipedashConfig,
  Pipeline,
  PipelineRun,
  PluginMetadata,
  ProviderConfig,
  ProviderSummary,
  RefreshResult,
  ReviewDecision,
  SetupStatus,
  StorageConfigResponse,
  StoragePathsResponse,
  TableSchema,
  TriggerParams,
  UnlockVaultResponse,
  ValidationResult,
  VaultStatusResponse,
  WorkflowParameter,
} from '../types'
import { getHttpStatus } from '../types/errors'
import { DEFAULT_RETRY_CONFIG, withRetry } from '../utils/retryLogic'

const API_BASE = import.meta.env.VITE_API_URL || withBasePath('/api/v1')

interface RequestOptions {
  timeoutMs?: number
  retry?: boolean
}

const REFRESH_REQUEST: RequestOptions = { timeoutMs: REFRESH_TIMEOUT_MS, retry: false }

const OPERATION_TIMEOUTS: Record<string, number> = {
  '/factory-reset': API_TIMEOUTS.EXTENDED,
  '/pipelines': API_TIMEOUTS.LONG,
  '/providers/preview': API_TIMEOUTS.UPLOAD,
  '/providers/organizations': API_TIMEOUTS.UPLOAD,
  default: API_TIMEOUTS.DEFAULT,
}

const getTimeoutFor = (method: string, path: string): number => {
  if (method === 'POST' && path === '/providers') {
    return API_TIMEOUTS.EXTENDED
  }

  for (const [key, timeout] of Object.entries(OPERATION_TIMEOUTS)) {
    if (path.includes(key)) {
      return timeout
    }
  }

  return OPERATION_TIMEOUTS.default
}

class ApiClient {
  private async fetchWithTimeout(
    url: string,
    options: RequestInit,
    timeoutMs: number = API_TIMEOUTS.DEFAULT
  ): Promise<Response> {
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs)

    try {
      const response = await fetch(url, {
        ...options,
        signal: controller.signal,
      })

      clearTimeout(timeoutId)

      return response
    } catch (error) {
      clearTimeout(timeoutId)
      if (error instanceof Error && error.name === 'AbortError') {
        throw new Error(`Request timeout after ${timeoutMs}ms`)
      }
      throw error
    }
  }

  private async request<T>(method: string, path: string, body?: unknown, options: RequestOptions = {}): Promise<T> {
    return withRetry(
      async () => {
        const token = getToken()
        const headers: Record<string, string> = {
          'Content-Type': 'application/json',
        }

        if (token) {
          headers.Authorization = `Bearer ${token}`
        }
        const res = await this.fetchWithTimeout(
          `${API_BASE}${path}`,
          {
            method,
            headers,
            body: body ? JSON.stringify(body) : undefined,
          },
          options.timeoutMs ?? getTimeoutFor(method, path)
        )

        if (res.ok) {
          useAuthStore.getState().resetFailures()
        }

        if (!res.ok) {
          if (res.status === 401) {
            useAuthStore.getState().incrementFailure()
          }
          const errorText = await res.text()
          const error = Object.assign(new Error(errorText || `HTTP ${res.status}`), { status: res.status })

          throw error
        }

        const text = await res.text()

        if (!text) {
          return undefined as T
        }

        try {
          return JSON.parse(text)
        } catch (parseError) {
          logger.error('API', 'Failed to parse response', parseError)
          throw new Error('Invalid response from server: response is not valid JSON')
        }
      },
      {
        ...DEFAULT_RETRY_CONFIG,
        maxAttempts: options.retry === false ? 1 : DEFAULT_RETRY_CONFIG.maxAttempts,
        shouldRetry: (error: Error) => {
          const status = getHttpStatus(error) ?? 0
          const isNetworkError = error instanceof TypeError
          const isTimeout = error.message.includes('timeout')
          const is5xxError = status >= 500 && status < 600
          const isRateLimit = status === 429

          return isNetworkError || isTimeout || is5xxError || isRateLimit
        },
      }
    )
  }

  private get<T>(path: string): Promise<T> {
    return this.request<T>('GET', path)
  }

  private post<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>('POST', path, body, options)
  }

  private put<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('PUT', path, body)
  }

  private delete<T>(path: string): Promise<T> {
    return this.request<T>('DELETE', path)
  }

  async addProvider(config: ProviderConfig): Promise<number> {
    const result = await this.post<{ id: number }>('/providers', {
      name: config.name,
      provider_type: config.provider_type,
      token: config.token,
      config: config.config,
      refresh_interval: config.refresh_interval,
    })

    return result.id
  }

  async listProviders(): Promise<ProviderSummary[]> {
    const result = await this.get<ProviderSummary[]>('/providers')

    return result || []
  }

  async getProvider(id: number): Promise<ProviderConfig & { id: number }> {
    const config = await this.get<ProviderConfig>(`/providers/${id}`)

    return { ...config, id }
  }

  async updateProvider(id: number, config: ProviderConfig): Promise<void> {
    await this.put(`/providers/${id}`, {
      name: config.name,
      token: config.token,
      config: config.config,
      refresh_interval: config.refresh_interval,
    })
  }

  async updateProviderRefreshInterval(id: number, refreshInterval: number): Promise<void> {
    await this.put(`/providers/${id}/refresh-interval`, {
      refresh_interval: refreshInterval,
    })
  }

  async removeProvider(id: number): Promise<void> {
    await this.delete(`/providers/${id}`)
  }

  async exportProviders(): Promise<string> {
    return this.get<string>('/providers/export')
  }

  async fetchProviderOrganizations(providerType: string, config: Record<string, string>): Promise<Organization[]> {
    return this.post<Organization[]>('/providers/organizations', {
      provider_type: providerType,
      config,
    })
  }

  async previewProviderPipelines(
    providerType: string,
    config: Record<string, string>,
    org?: string,
    search?: string,
    page?: number,
    pageSize?: number
  ): Promise<PaginatedAvailablePipelines> {
    return this.post<PaginatedAvailablePipelines>('/providers/preview', {
      provider_type: providerType,
      config,
      org: org ?? null,
      search: search ?? null,
      page,
      page_size: pageSize,
    })
  }

  async getProviderFieldOptions(
    providerType: string,
    fieldKey: string,
    config: Record<string, string>
  ): Promise<string[]> {
    return this.post<string[]>('/providers/field-options', {
      provider_type: providerType,
      field_key: fieldKey,
      config,
    })
  }

  async getProviderPermissions(providerId: number): Promise<PermissionStatus | null> {
    return this.get<PermissionStatus | null>(`/providers/${providerId}/permissions`)
  }

  async getProviderFeatures(providerId: number): Promise<FeatureAvailability[]> {
    return this.get<FeatureAvailability[]>(`/providers/${providerId}/features`)
  }

  async getProviderTableSchema(providerId: number): Promise<TableSchema> {
    return this.get<TableSchema>(`/providers/${providerId}/table-schema`)
  }

  async validateProviderCredentials(providerType: string, config: Record<string, string>): Promise<ValidationResult> {
    return this.post<ValidationResult>('/providers/validate', {
      provider_type: providerType,
      config,
    })
  }

  async checkProviderPermissions(providerType: string, config: Record<string, string>): Promise<PermissionCheckResult> {
    return this.post<PermissionCheckResult>('/providers/permissions/check', {
      provider_type: providerType,
      config,
    })
  }

  async getCachedPipelines(providerId?: number): Promise<Pipeline[]> {
    const params = providerId ? `?provider_id=${providerId}` : ''
    const result = await this.get<Pipeline[]>(`/pipelines/cached${params}`)

    return result || []
  }

  async fetchRunHistory(pipelineId: string, page?: number, pageSize?: number): Promise<PaginatedRunHistory> {
    const params = new URLSearchParams()

    if (page) {
      params.set('page', String(page))
    }
    if (pageSize) {
      params.set('page_size', String(pageSize))
    }
    const queryString = params.toString() ? `?${params.toString()}` : ''

    const result = await this.get<PaginatedRunHistory>(
      `/pipelines/${encodeURIComponent(pipelineId)}/runs${queryString}`
    )

    return result
  }

  async triggerPipeline(params: TriggerParams): Promise<string> {
    const result = await this.post<{ run_id: string }>(`/pipelines/${encodeURIComponent(params.workflow_id)}/trigger`, {
      workflow_id: params.workflow_id,
      inputs: params.inputs,
    })

    return result.run_id
  }

  async getWorkflowParameters(workflowId: string): Promise<WorkflowParameter[]> {
    return this.get<WorkflowParameter[]>(`/pipelines/${encodeURIComponent(workflowId)}/workflow-params`)
  }

  async refreshProvider(id: number): Promise<RefreshResult> {
    return this.post<RefreshResult>(`/providers/${id}/refresh`, undefined, REFRESH_REQUEST)
  }

  async refreshAll(): Promise<RefreshResult[]> {
    return this.post<RefreshResult[]>('/refresh/all', undefined, REFRESH_REQUEST)
  }

  async updateGeneralSettings(update: GeneralConfig): Promise<void> {
    await this.put('/settings/general', update)
  }

  async getWorkflowRunDetails(pipelineId: string, runNumber: number): Promise<PipelineRun> {
    return this.get<PipelineRun>(`/pipelines/${encodeURIComponent(pipelineId)}/runs/${runNumber}`)
  }

  async cancelPipelineRun(pipelineId: string, runNumber: number): Promise<void> {
    await this.post(`/pipelines/${encodeURIComponent(pipelineId)}/runs/${runNumber}/cancel`)
  }

  async reviewPipelineRun(pipelineId: string, runNumber: number, decision: ReviewDecision): Promise<void> {
    await this.post(`/pipelines/${encodeURIComponent(pipelineId)}/runs/${runNumber}/review`, { decision })
  }

  async openUrl(url: string): Promise<void> {
    window.open(url, '_blank', 'noopener,noreferrer')
  }

  async getAvailablePlugins(): Promise<PluginMetadata[]> {
    return this.get<PluginMetadata[]>('/plugins')
  }

  async clearRunHistoryCache(pipelineId: string): Promise<void> {
    await this.delete(`/cache/run-history/${encodeURIComponent(pipelineId)}`)
  }

  async getCacheStats(): Promise<{
    pipelines_count: number
    run_history_count: number
    workflow_params_count: number
    metrics_count: number
  }> {
    return this.get('/cache/stats')
  }

  async clearPipelinesCache(): Promise<number> {
    const result = await this.delete<{ cleared: number }>('/cache/pipelines')

    return result.cleared
  }

  async clearAllRunHistoryCaches(): Promise<void> {
    await this.delete('/cache/run-history')
  }

  async clearWorkflowParamsCache(): Promise<void> {
    await this.delete('/cache/workflow-params')
  }

  async clearAllCaches(): Promise<void> {
    await this.delete('/cache')
  }

  async getGlobalMetricsConfig(): Promise<GlobalMetricsConfig> {
    return this.get<GlobalMetricsConfig>('/metrics/config')
  }

  async updateGlobalMetricsConfig(enabled: boolean, defaultRetentionDays: number): Promise<void> {
    await this.put('/metrics/config', {
      enabled,
      default_retention_days: defaultRetentionDays,
    })
  }

  async getPipelineMetricsConfig(pipelineId: string): Promise<MetricsConfig> {
    return this.get<MetricsConfig>(`/metrics/pipelines/${encodeURIComponent(pipelineId)}/config`)
  }

  async updatePipelineMetricsConfig(pipelineId: string, enabled: boolean, retentionDays: number): Promise<void> {
    await this.put(`/metrics/pipelines/${encodeURIComponent(pipelineId)}/config`, {
      enabled,
      retention_days: retentionDays,
    })
  }

  async queryPipelineMetrics(
    pipelineId?: string,
    metricType?: MetricType,
    startDate?: string,
    endDate?: string,
    limit?: number
  ): Promise<MetricEntry[]> {
    return this.post<MetricEntry[]>(`/metrics/pipelines/${encodeURIComponent(pipelineId || 'all')}/query`, {
      pipeline_id: pipelineId ?? null,
      metric_type: metricType ?? null,
      start_date: startDate ?? null,
      end_date: endDate ?? null,
      limit: limit ?? null,
    })
  }

  async queryAggregatedMetrics(
    metricType: MetricType,
    aggregationPeriod: AggregationPeriod,
    aggregationType?: AggregationType,
    pipelineId?: string,
    startDate?: string,
    endDate?: string,
    limit?: number
  ): Promise<AggregatedMetrics> {
    return this.post<AggregatedMetrics>('/metrics/aggregated', {
      pipeline_id: pipelineId ?? null,
      metric_type: metricType,
      aggregation_period: aggregationPeriod,
      aggregation_type: aggregationType ?? null,
      start_date: startDate ?? null,
      end_date: endDate ?? null,
      limit: limit ?? null,
    })
  }

  async getMetricsStorageStats(): Promise<MetricsStats> {
    return this.get<MetricsStats>('/metrics/storage/stats')
  }

  async flushPipelineMetrics(pipelineId?: string): Promise<number> {
    const result = await this.post<{ flushed: number }>('/metrics/flush', {
      pipeline_id: pipelineId ?? null,
    })

    return result.flushed
  }

  async getTablePreferences(providerId: number, tableId: string): Promise<string | null> {
    return this.get<string | null>(`/preferences/table/${providerId}/${encodeURIComponent(tableId)}`)
  }

  async saveTablePreferences(providerId: number, tableId: string, preferencesJson: string): Promise<void> {
    await this.put(`/preferences/table/${providerId}/${encodeURIComponent(tableId)}`, {
      preferences_json: preferencesJson,
    })
  }

  async getDefaultTablePreferences(providerId: number, tableId: string): Promise<string> {
    const defaults = await this.get<unknown>(`/preferences/table/${providerId}/${encodeURIComponent(tableId)}/default`)

    return JSON.stringify(defaults)
  }

  async checkSetupStatus(): Promise<SetupStatus> {
    return this.get<SetupStatus>('/setup/status')
  }

  async createInitialConfig(config: PipedashConfig, vaultPassword?: string): Promise<void> {
    await this.post('/setup/config', {
      config,
      vault_password: vaultPassword,
    })
  }

  async bootstrapApp(): Promise<void> {
    return Promise.resolve()
  }

  async getStorageConfig(): Promise<StorageConfigResponse> {
    return this.get<StorageConfigResponse>('/storage/config')
  }

  async getVaultPasswordStatus(): Promise<{ is_set: boolean; env_var_name: string }> {
    return this.get<{ is_set: boolean; env_var_name: string }>('/storage/vault-password-status')
  }

  async getVaultStatus(): Promise<VaultStatusResponse> {
    return this.get('/vault/status')
  }

  async unlockVault(password: string): Promise<UnlockVaultResponse> {
    return this.post('/vault/unlock', { password })
  }

  async lockVault(): Promise<UnlockVaultResponse> {
    return this.post('/vault/lock')
  }

  async getConfigContent(): Promise<ConfigContentResponse> {
    return this.get<ConfigContentResponse>('/storage/config/content')
  }

  async saveConfigContent(content: string): Promise<void> {
    await this.put('/storage/config/content', { content })
  }

  async analyzeConfig(content: string): Promise<ConfigAnalysisResponse> {
    return this.post<ConfigAnalysisResponse>('/storage/config/analyze', {
      new_content: content,
    })
  }

  async getStoragePaths(): Promise<StoragePathsResponse> {
    return this.get<StoragePathsResponse>('/storage/paths')
  }

  async getDefaultDataDir(): Promise<string> {
    return this.get<string>('/storage/default-data-dir')
  }

  async getEffectiveDataDir(config: PipedashConfig): Promise<string> {
    return this.post<string>('/storage/effective-data-dir', config)
  }

  async checkDatabaseExists(config: PipedashConfig): Promise<{ exists: boolean; path: string }> {
    return this.post<{ exists: boolean; path: string }>('/storage/check-database', config)
  }

  async saveStorageConfig(config: PipedashConfig, tokenPassword?: string): Promise<void> {
    const options: MigrationOptions = {
      migrate_tokens: !!tokenPassword,
      migrate_cache: true,
      dry_run: false,
      token_password: tokenPassword,
    }

    const plan = await this.post<MigrationPlan>('/storage/migration/plan', {
      target_config: config,
      options,
    })

    const result = await this.post<MigrationResult>('/storage/migration/execute', {
      plan,
      options,
    })

    if (!result.success) {
      throw new Error(`Migration failed: ${result.errors.join(', ')}`)
    }
  }

  async testStorageConnection(config: PipedashConfig): Promise<{
    success: boolean
    message: string
  }> {
    return this.post<{ success: boolean; message: string }>('/storage/test-connection', config)
  }

  async planStorageMigration(targetConfig: PipedashConfig, options?: MigrationOptions): Promise<MigrationPlan> {
    return this.post<MigrationPlan>('/storage/migration/plan', {
      target_config: targetConfig,
      options: options ?? {},
    })
  }

  async executeStorageMigration(plan: MigrationPlan, options?: MigrationOptions): Promise<MigrationResult> {
    return this.post<MigrationResult>('/storage/migration/execute', {
      plan,
      options: options ?? {},
    })
  }

  async executeMigration(plan: MigrationPlan, options?: MigrationOptions): Promise<MigrationResult> {
    return this.executeStorageMigration(plan, options)
  }

  async factoryReset(): Promise<{
    providers_removed: number
    caches_cleared: boolean
    tokens_cleared: boolean
    metrics_cleared: boolean
  }> {
    return this.post('/factory-reset')
  }

  async restartApp(): Promise<void> {
    window.location.reload()
  }
}

export const apiService = new ApiClient()
