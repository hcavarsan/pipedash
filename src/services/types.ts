import type { RefreshResult } from '../types/health'
import type { LogLinesPage, LogSearchQuery, LogSearchResult, RunLogDocument, RunLogTree } from '../types/logs'
import type {
  GlobalMetricsConfig,
  MetricsConfig,
  MetricsDashboard,
  MetricsDashboardQuery,
  MetricsStats,
} from '../types/metrics'
import type { NotificationTarget, PairedDevice, PairingCode } from '../types/native'
import type { NotificationPreferenceSnapshot, PreferenceChange } from '../types/notification_preferences'
import type {
  MobileDevicesResponse,
  NotificationCenterChange,
  NotificationCenterSnapshot,
  NotificationPreferences,
  PushRelaySettings,
} from '../types/notifications'
import type {
  Organization,
  PaginatedAvailablePipelines,
  Pipeline,
  PipelineRun,
  ReviewDecision,
  TriggerDraft,
  TriggerParams,
  TriggerResult,
} from '../types/pipelines'
import type { PermissionCheckResult, PluginMetadata, TableSchema } from '../types/plugins'
import type { ProviderDraft, ProviderDraftResult } from '../types/provider_draft'
import type {
  ProviderConfig,
  ProviderImportResult,
  ProviderSummary,
  SignInStart,
  SignInStatus,
} from '../types/providers'
import type { CancellationBatch, CancellationTargets, RunTarget } from '../types/run_action_batch'
import type { RunHistoryPage, RunHistoryQuery } from '../types/run_history'
import type { SetupApplied, SetupDraft, SetupDraftResult } from '../types/setup'
import type {
  ConfigAnalysisResponse,
  ConfigContentResponse,
  GeneralConfig,
  MigrationOptions,
  MigrationPlan,
  MigrationResult,
  PipedashConfig,
  SetupStatus,
  StorageConfigResponse,
  StoragePathsResponse,
} from '../types/storage'
import type { UnlockVaultResponse, VaultStatusResponse } from '../types/vault'
import type {
  CreateViewInput,
  LegacyImportResult,
  LegacyMarks,
  PinKind,
  PinnedItem,
  PipelineNavigation,
  PipelineView,
  PipelineViewPage,
  PipelineViewQuery,
  ViewEditorOptions,
  ViewMember,
  ViewMembershipChange,
  ViewPatch,
  ViewRef,
  ViewSelectionEntry,
} from '../types/views'

export interface ProvidersService {
  listProviders(): Promise<ProviderSummary[]>
  getProvider(id: number): Promise<ProviderConfig & { id: number }>
  updateProviderRefreshInterval(id: number, refreshInterval: number): Promise<void>
  removeProvider(id: number): Promise<void>
  exportProviders(): Promise<string>
  importProviders(content: string): Promise<ProviderImportResult>
  fetchProviderOrganizations(providerType: string, config: Record<string, string>): Promise<Organization[]>
  previewProviderPipelines(
    providerType: string,
    config: Record<string, string>,
    org?: string,
    search?: string,
    page?: number,
    pageSize?: number
  ): Promise<PaginatedAvailablePipelines>
  getProviderFieldOptions(providerType: string, fieldKey: string, config: Record<string, string>): Promise<string[]>
  getProviderTableSchema(providerId: number): Promise<TableSchema>
  startSignIn(providerType: string, config: Record<string, string>): Promise<SignInStart>
  startProviderSignIn(id: number): Promise<SignInStart>
  reconnectProvider(id: number, flowId: string): Promise<void>
  signInStatus(id: string): Promise<SignInStatus>
  completeSignIn(id: string, redirectUrl: string): Promise<SignInStatus>
  cancelSignIn(id: string): Promise<void>
  checkProviderPermissions(providerType: string, config: Record<string, string>): Promise<PermissionCheckResult>
  getAvailablePlugins(): Promise<PluginMetadata[]>
  validateProviderDraft(draft: ProviderDraft): Promise<ProviderDraftResult>
  saveProviderDraft(draft: ProviderDraft): Promise<number>
  getMatchingPipelineIds(
    providerType: string,
    config: Record<string, string>,
    organization?: string,
    search?: string
  ): Promise<string[]>
}

export interface RunsService {
  getCachedPipelines(providerId?: number): Promise<Pipeline[]>
  fetchRunHistory(pipelineId: string, query: RunHistoryQuery): Promise<RunHistoryPage>
  preparePipelineTrigger(
    pipelineId: string,
    rerunOf: number | null,
    inputs: Record<string, unknown> | null
  ): Promise<TriggerDraft>
  triggerPipeline(params: TriggerParams): Promise<TriggerResult>
  getCancellationTargets(pipelineIds: string[]): Promise<CancellationTargets>
  cancelPipelineRuns(targets: RunTarget[]): Promise<CancellationBatch>
  refreshProvider(id: number): Promise<RefreshResult>
  refreshAll(): Promise<RefreshResult[]>
  updateGeneralSettings(update: GeneralConfig): Promise<void>
  getWorkflowRunDetails(pipelineId: string, runNumber: number): Promise<PipelineRun>
  cancelPipelineRun(pipelineId: string, runNumber: number): Promise<void>
  forceCancelPipelineRun(pipelineId: string, runNumber: number): Promise<void>
  reviewPipelineRun(pipelineId: string, runNumber: number, decision: ReviewDecision): Promise<void>
  rerunFailedJobs(pipelineId: string, runNumber: number): Promise<void>
  rerunRun(pipelineId: string, runNumber: number): Promise<TriggerResult>
  enablePipeline(pipelineId: string): Promise<void>
}

export interface LogsService {
  getCacheStats(): Promise<{
    pipelines_count: number
    run_history_count: number
    workflow_params_count: number
    metrics_count: number
    run_logs_bytes: number
  }>
  clearPipelinesCache(): Promise<number>
  clearAllRunHistoryCaches(): Promise<void>
  clearWorkflowParamsCache(): Promise<void>
  clearAllCaches(): Promise<void>
  getRunLogTree(pipelineId: string, runNumber: number): Promise<RunLogTree>
  openRunLog(pipelineId: string, runNumber: number, nodeId: string, retry: boolean): Promise<RunLogDocument>
  readRunLogLines(
    pipelineId: string,
    runNumber: number,
    nodeId: string,
    revision: number,
    start: number,
    count: number,
    maxLineUnits: number
  ): Promise<LogLinesPage>
  searchRunLog(
    pipelineId: string,
    runNumber: number,
    nodeId: string,
    revision: number,
    sessionId: string,
    query: LogSearchQuery,
    signal?: AbortSignal
  ): Promise<LogSearchResult>
  clearRunLogsCache(): Promise<void>
  exportRunLog(
    pipelineId: string,
    runNumber: number,
    nodeId: string,
    revision: number,
    fileName: string
  ): Promise<boolean>
}

export interface MetricsService {
  getGlobalMetricsConfig(): Promise<GlobalMetricsConfig>
  updateGlobalMetricsConfig(enabled: boolean, defaultRetentionDays: number): Promise<void>
  getPipelineMetricsConfig(pipelineId: string): Promise<MetricsConfig>
  updatePipelineMetricsConfig(pipelineId: string, enabled: boolean, retentionDays: number): Promise<void>
  getMetricsDashboard(query: MetricsDashboardQuery): Promise<MetricsDashboard>
  getMetricsStorageStats(): Promise<MetricsStats>
  flushPipelineMetrics(pipelineId?: string): Promise<number>
}

export interface ViewsService {
  getTablePreferences(providerId: number, tableId: string): Promise<string | null>
  saveTablePreferences(providerId: number, tableId: string, preferencesJson: string): Promise<void>
  getDefaultTablePreferences(providerId: number, tableId: string): Promise<string>
  listPipelineViews(): Promise<PipelineView[]>
  getPipelineViewPage(query: PipelineViewQuery): Promise<PipelineViewPage>
  createPipelineView(input: CreateViewInput): Promise<PipelineView>
  updatePipelineView(id: ViewRef, patch: ViewPatch): Promise<PipelineView>
  deletePipelineView(id: ViewRef): Promise<void>
  reorderPipelineViews(order: ViewRef[]): Promise<void>
  listPipelineViewMembers(id: ViewRef): Promise<ViewMember[]>
  setPipelineViewMembership(id: ViewRef, pipelineId: string, member: boolean): Promise<void>
  setPipelineHidden(pipelineId: string, hidden: boolean): Promise<void>
  listPins(): Promise<PinnedItem[]>
  setPin(kind: PinKind, ref: string, pinned: boolean): Promise<void>
  importLegacyPipelineMarks(marks: LegacyMarks): Promise<LegacyImportResult>
  getViewSelection(pipelineIds: string[]): Promise<ViewSelectionEntry[]>
  setViewMemberships(id: ViewRef, change: ViewMembershipChange): Promise<ViewSelectionEntry[]>
  getViewEditorOptions(view: string | null): Promise<ViewEditorOptions>
  getPipelineNavigation(search?: string): Promise<PipelineNavigation>
}

export interface StorageService {
  checkSetupStatus(): Promise<SetupStatus>
  prepareSetupDraft(draft: SetupDraft, firstRun: boolean): Promise<SetupDraftResult>
  applySetupDraft(draft: SetupDraft, firstRun: boolean): Promise<SetupApplied>
  getStorageConfig(): Promise<StorageConfigResponse>
  getVaultPasswordStatus(): Promise<{ is_set: boolean; env_var_name: string }>
  getVaultStatus(): Promise<VaultStatusResponse>
  unlockVault(password: string): Promise<UnlockVaultResponse>
  lockVault(): Promise<UnlockVaultResponse>
  getConfigContent(): Promise<ConfigContentResponse>
  saveConfigContent(content: string): Promise<void>
  analyzeConfig(content: string): Promise<ConfigAnalysisResponse>
  getStoragePaths(): Promise<StoragePathsResponse>
  getDefaultDataDir(): Promise<string>
  testStorageConnection(config: PipedashConfig): Promise<{ success: boolean; message: string }>
  executeMigration(plan: MigrationPlan, options?: MigrationOptions): Promise<MigrationResult>
  factoryReset(): Promise<{
    providers_removed: number
    caches_cleared: boolean
    tokens_cleared: boolean
    metrics_cleared: boolean
  }>
}

export interface DevicesService {
  listPairedDevices(): Promise<PairedDevice[]>
  createPairingCode(): Promise<PairingCode>
  removePairedDevice(id: string): Promise<void>
  getPushRelay(): Promise<PushRelaySettings>
  setPushRelay(enabled: boolean): Promise<PushRelaySettings>
}

export interface NotificationsService {
  updateNotificationCenter(
    snapshot: NotificationCenterSnapshot,
    change: NotificationCenterChange
  ): Promise<NotificationCenterSnapshot>
  changeNotificationPreference(
    change: PreferenceChange,
    current?: NotificationPreferences
  ): Promise<NotificationPreferenceSnapshot>
}

export interface PlatformService {
  openUrl(url: string): Promise<void>
  bootstrapApp(): Promise<void>
  restartApp(): Promise<void>
}

export interface PipedashService
  extends ProvidersService,
    RunsService,
    LogsService,
    MetricsService,
    ViewsService,
    StorageService,
    DevicesService,
    NotificationsService,
    PlatformService {}

export interface ServerVersionService {
  getServerVersion(): Promise<string>
}

export interface PushService {
  listMobileDevices(): Promise<MobileDevicesResponse>
  removeMobileDevice(id: string): Promise<void>
  testMobileDevice(id: string): Promise<void>
}

export interface NativeOnlyService {
  setRefreshMode(mode: 'active' | 'idle'): Promise<void>
  showSystemNotification(title: string, body: string, target: NotificationTarget): Promise<void>
}
