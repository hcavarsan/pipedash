export type PipelineStatus = 'success' | 'failed' | 'running' | 'pending' | 'cancelled' | 'skipped'

export interface BuildWatchSnapshot {
  sessionId: string
  pipelineId: string
  pipelineName: string
  providerName: string
  providerType: string
  runNumber: number
  branch: string | null
  status: PipelineStatus
  startedAt: number
  concludedAt: number | null
  updatedAt: number
  monitoring: 'watching' | 'completed' | 'interrupted' | 'stopped'
  url: string
}

export interface BuildWatchStatus {
  supported: boolean
  available: boolean
  reason: string | null
  watch: BuildWatchSnapshot | null
}

export type ReviewDecision = 'approve' | 'reject'

export interface AvailablePipeline {
  id: string
  name: string
  description: string | null
  organization: string | null
  repository: string | null
}

export interface Organization {
  id: string
  name: string
  description: string | null
}

export interface Pipeline {
  id: string
  provider_id: number
  provider_type: string
  name: string
  status: PipelineStatus
  last_run: string | null
  last_updated: string
  repository: string
  branch: string | null
  workflow_file: string | null
  metadata?: Record<string, unknown>
  overdue_run?: string | null
}

export interface PipelineRun {
  id: string
  pipeline_id: string
  run_number: number
  status: PipelineStatus
  started_at: string
  concluded_at: string | null
  duration_seconds: number | null
  logs_url: string
  commit_sha: string | null
  commit_message: string | null
  branch: string | null
  actor: string | null
  inputs?: Record<string, unknown>
  metadata?: Record<string, unknown>
  [key: string]: unknown
}

export interface PaginatedResponse<T> {
  items: T[]
  page: number
  page_size: number
  total_count: number
  total_pages: number
  has_more: boolean
}

export type PaginatedAvailablePipelines = PaginatedResponse<AvailablePipeline>

export interface PaginatedRunHistory {
  runs: PipelineRun[]
  total_count: number
  has_more: boolean
  is_complete: boolean
  page: number
  page_size: number
  total_pages: number
}

export interface ProviderConfig {
  id?: number
  name: string
  provider_type: string
  token: string
  config: Record<string, string>
  refresh_interval: number
}

export type FetchStatus = 'success' | 'error' | 'never'

export type HealthLevel = 'operational' | 'maintenance' | 'degraded' | 'partial_outage' | 'major_outage'

export type FailureKind =
  | 'network'
  | 'timeout'
  | 'tls'
  | 'rate_limited'
  | 'unavailable'
  | 'auth'
  | 'permission'
  | 'config'
  | 'not_found'
  | 'other'

export type SignalKind = 'status_page' | 'instance' | 'connection' | 'rate_limit'

export type RefreshState = 'ok' | 'failed' | 'pending' | 'recent' | 'not_found'

export interface ComponentHealth {
  name: string
  level: HealthLevel
  detail: string | null
}

export interface Incident {
  id: string
  name: string
  level: HealthLevel
  phase: string
  latest_update: string | null
  url: string | null
  started_at: string | null
  updated_at: string
}

export interface Maintenance {
  id: string
  name: string
  in_progress: boolean
  starts_at: string | null
  ends_at: string | null
  url: string | null
}

export interface HealthSignal {
  kind: SignalKind
  episode_id: string
  level: HealthLevel
  evidence: 'fresh' | 'stale'
  observed_at: string
  title: string
  detail: string | null
  reported_by: string
  url: string | null
  components: ComponentHealth[]
  incidents: Incident[]
  maintenances: Maintenance[]
  updated_at: string
}

export interface StatusPageRef {
  name: string
  url: string
  checked_at: string | null
  error: string | null
  components: ComponentHealth[]
  other_incidents: Incident[]
}

export interface ProviderHealth {
  level: HealthLevel
  signals: HealthSignal[]
  next_attempt_at: string | null
  in_flight: boolean
  status_page: StatusPageRef | null
}

export interface RefreshResult {
  provider_id: number
  state: RefreshState
  kind: FailureKind | null
  message: string | null
}

export interface ProviderHealthAlert {
  alert_id: string
  notification_key: string
  provider_ids: number[]
  level: HealthLevel
  title: string
  body: string
  link: { url: string; kind: 'incident' | 'status_page' } | null
  recovered: boolean
}

export interface ProviderSummary {
  id: number
  name: string
  provider_type: string
  icon: string | null
  pipeline_count: number
  last_updated: string | null
  refresh_interval: number
  configured_repositories: string[]
  last_fetch_status: FetchStatus
  last_fetch_error: string | null
  last_fetch_error_kind: FailureKind | null
  last_fetch_at: string | null
  paused_until: string | null
  health: ProviderHealth
}

export interface ProviderImportResult {
  added: string[]
  skipped: string[]
}

export interface TriggerParams {
  workflow_id: string
  inputs?: Record<string, unknown>
}

interface PluginCapabilities {
  pipelines: boolean
  pipeline_runs: boolean
  trigger: boolean
  cancel: boolean
  agents: boolean
  artifacts: boolean
  queues: boolean
  custom_tables: boolean
  retains_run_history: boolean
}

type ConfigFieldType = 'Text' | 'Password' | 'TextArea' | 'Boolean' | 'Select' | 'MultiSelect' | 'Number'

export interface ConfigField {
  key: string
  label: string
  description: string | null
  field_type: ConfigFieldType
  required: boolean
  default_value: string | number | boolean | null
  options: string[] | null
  validation_regex: string | null
  validation_message: string | null
}

interface ConfigSchema {
  fields: ConfigField[]
}

export type ColumnDataType =
  | 'String'
  | 'Number'
  | 'DateTime'
  | 'Duration'
  | 'Status'
  | 'Badge'
  | 'Url'
  | 'Json'
  | 'Boolean'
  | { Custom: string }

export type CellRenderer =
  | 'Text'
  | 'Badge'
  | 'DateTime'
  | 'Duration'
  | 'StatusBadge'
  | 'Commit'
  | 'Avatar'
  | 'TruncatedText'
  | 'Link'
  | 'JsonViewer'
  | { Custom: string }

export type ColumnVisibility =
  | 'Always'
  | 'WhenPresent'
  | { WhenCapability: string }
  | { Conditional: { field: string; equals: unknown } }

export interface ColumnDefinition {
  id: string
  label: string
  description: string | null
  field_path: string
  data_type: ColumnDataType
  renderer: CellRenderer
  visibility: ColumnVisibility
  default_visible: boolean
  width: number | null
  sortable: boolean
  filterable: boolean
}

export interface TableDefinition {
  id: string
  name: string
  description: string | null
  columns: ColumnDefinition[]
  default_sort_column: string | null
  default_sort_direction: string | null
}

export interface TableSchema {
  tables: TableDefinition[]
}

export interface PluginMetadata {
  name: string
  provider_type: string
  version: string
  description: string
  author: string | null
  icon: string | null
  config_schema: ConfigSchema
  table_schema: TableSchema
  capabilities: PluginCapabilities
  required_permissions: Permission[]
  features: Feature[]
}

export interface Permission {
  name: string
  description: string
  required: boolean
}

export interface PermissionCheck {
  permission: Permission
  granted: boolean
}

export interface PermissionStatus {
  permissions: PermissionCheck[]
  all_granted: boolean
  checked_at: string
  metadata?: Record<string, string>
}

export interface Feature {
  id: string
  name: string
  description: string
  required_permissions: string[]
}

export interface FeatureAvailability {
  feature: Feature
  available: boolean
  missing_permissions: string[]
}

export interface ValidationResult {
  valid: boolean
  error: string | null
}

export interface PermissionCheckResult {
  permission_status: PermissionStatus | null
  features: FeatureAvailability[]
}

export interface WorkflowParameter {
  name: string
  label: string | null
  description: string | null
  type: 'string' | 'boolean' | 'choice' | 'number'
  default?: string | number | boolean | null
  options?: string[]
  required: boolean
}

export type MetricType = 'run_duration' | 'success_rate' | 'run_frequency'

export type AggregationPeriod = 'hourly' | 'daily' | 'weekly' | 'monthly'

export type AggregationType = 'avg' | 'sum' | 'min' | 'max' | 'p95' | 'p99'

export interface GlobalMetricsConfig {
  enabled: boolean
  default_retention_days: number
  updated_at: string
}

export interface MetricsConfig {
  pipeline_id: string
  enabled: boolean
  retention_days: number
  created_at: string
  updated_at: string
}

export interface MetricMetadata {
  status?: string
  branch?: string
  repository?: string
  actor?: string
}

export interface MetricEntry {
  id: number
  pipeline_id: string
  timestamp: string
  metric_type: MetricType
  value: number
  metadata: MetricMetadata | null
  created_at: string
}

export interface AggregatedMetric {
  timestamp: string
  value: number
  count: number
  min: number | null
  max: number | null
  avg: number
}

export interface AggregatedMetrics {
  metrics: AggregatedMetric[]
  total_count: number
  metric_type: MetricType
  aggregation_period: AggregationPeriod
}

export interface PipelineMetricsStats {
  pipeline_id: string
  pipeline_name: string
  metrics_count: number
  oldest_metric: string | null
  newest_metric: string | null
}

export interface MetricsStats {
  total_metrics_count: number
  estimated_size_bytes: number
  estimated_size_mb: number
  last_cleanup_at: string | null
  updated_at: string
  by_pipeline: PipelineMetricsStats[]
}

export type StorageBackendType = 'sqlite' | 'postgres'

export interface PostgresSettings {
  connection_string: string
}

export interface StorageConfig {
  data_dir: string
  backend: StorageBackendType
  postgres?: PostgresSettings
}

export interface GeneralConfig {
  metrics_enabled: boolean
  default_refresh_interval: number
  status_pages_enabled: boolean
  proxy_url: string
}

export interface ServerConfig {
  bind_addr: string
  cors_allow_all: boolean
}

export interface PipedashConfig {
  general: GeneralConfig
  server: ServerConfig
  storage: StorageConfig
  providers?: ProviderFileConfig[]
}

export interface ProviderFileConfig {
  name: string
  type: string
  token: string
  refresh_interval?: number
  config?: Record<string, string>
}

export interface StorageConfigResponse {
  config: PipedashConfig
  summary: string
}

export type MigrationStep =
  | 'ValidateTarget'
  | 'MigrateTokens'
  | 'MigrateConfigs'
  | 'MigrateCache'
  | 'VerifyMigration'
  | 'UpdateConfig'

export interface MigrationPlan {
  from: PipedashConfig
  to: PipedashConfig
  steps: MigrationStep[]
  migrate_tokens: boolean
  migrate_configs: boolean
  migrate_cache: boolean
  backend_changed: boolean
  data_dir_changed: boolean
  created_at: string
}

export interface MigrationOptions {
  migrate_tokens: boolean
  migrate_cache: boolean
  token_password?: string
  dry_run: boolean
}

export interface MigrationResult {
  success: boolean
  steps_completed: MigrationStep[]
  errors: string[]
  duration_ms: number
  stats: {
    providers_migrated: number
    tokens_migrated: number
    cache_entries_migrated: number
    metrics_migrated: number
    permissions_migrated: number
  }
  provider_id_mapping: Record<number, number>
}

export interface ConfigIssue {
  field: string
  message: string
  code: string
}

export interface ConnectionTestResult {
  success: boolean
  message: string
  latency_ms?: number
}

export interface MigrationStatsPreview {
  providers_count: number
  tokens_count: number
  cache_entries_count: number
}

export interface ConfigAnalysisResponse {
  valid: boolean
  errors: ConfigIssue[]
  warnings: ConfigIssue[]
  migration_plan?: MigrationPlan
  postgres_connection?: ConnectionTestResult
  stats?: MigrationStatsPreview
}

export type {
  ModalBaseProps,
  PipelineComponentProps,
} from './components'
export { createError, toPipedashError } from './errors'

export interface SetupStatus {
  config_exists: boolean
  config_valid: boolean
  validation_errors: string[]
  needs_setup: boolean
  needs_migration: boolean
  database_exists?: boolean
  database_path?: string
}

export interface ConfigContentResponse {
  content: string
  path: string
}

export interface StoragePathsResponse {
  config_file: string
  pipedash_db: string
  metrics_db: string
  data_dir: string
  cache_dir: string
  vault_path: string
}

export type PasswordSource = 'env_var' | 'keyring' | 'session' | 'none'

export type Biometry = 'none' | 'touchId' | 'faceId'

export interface AppLockStatus {
  supported: boolean
  biometry: Biometry
  biometry_available: boolean
  enabled: boolean
  lock_on_background: boolean
  locked: boolean
}

export interface VaultStatusResponse {
  is_unlocked: boolean
  password_source: PasswordSource
  backend: StorageBackendType
  requires_password: boolean
  is_first_time?: boolean
  app_lock?: AppLockStatus
}

export interface UnlockVaultResponse {
  success: boolean
  message: string
  reason?: 'cancelled'
}

export interface ExportFileResult {
  path: string
  shared: boolean
}

export interface NotificationTarget {
  key: string
  pipelineId?: string
  providerId?: number
  runNumber?: number
}

export type NativeIntent =
  | ({ type: 'notification'; action: string } & Partial<NotificationTarget>)
  | { type: 'link'; path: string; runNumber?: number }

export interface PushServer {
  id: string
  url: string
  label: string
  paired_at: string
}

export interface PushStatus {
  supported: boolean
  relay_configured: boolean
  permission: 'granted' | 'denied' | 'prompt'
  servers: PushServer[]
}

export interface MobileDevice {
  id: string
  device_name: string
  platform: 'ios'
  created_at: string
  last_success_at: string | null
}

export interface MobileDevicesResponse {
  relay_configured: boolean
  subscriptions: MobileDevice[]
}
