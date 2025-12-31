export type PipelineStatus =
  | 'success'
  | 'failed'
  | 'running'
  | 'pending'
  | 'cancelled'
  | 'skipped';

export interface AvailablePipeline {
  id: string;
  name: string;
  description: string | null;
  organization: string | null;
  repository: string | null;
}

export interface Organization {
  id: string;
  name: string;
  description: string | null;
}

export interface Pipeline {
  id: string;
  provider_id: number;
  provider_type: string;
  name: string;
  status: PipelineStatus;
  last_run: string | null;
  last_updated: string;
  repository: string;
  branch: string | null;
  workflow_file: string | null;
  metadata?: Record<string, any>;
}

export interface PipelineRun {
  id: string;
  pipeline_id: string;
  run_number: number;
  status: PipelineStatus;
  started_at: string;
  concluded_at: string | null;
  duration_seconds: number | null;
  logs_url: string;
  commit_sha: string | null;
  commit_message: string | null;
  branch: string | null;
  actor: string | null;
  inputs?: Record<string, any>;
  metadata?: Record<string, any>;
  [key: string]: unknown;
}

export interface PaginatedResponse<T> {
  items: T[];
  page: number;
  page_size: number;
  total_count: number;
  total_pages: number;
  has_more: boolean;
}

export type PaginatedAvailablePipelines = PaginatedResponse<AvailablePipeline>;

export interface PaginatedRunHistory {
  runs: PipelineRun[];
  total_count: number;
  has_more: boolean;
  is_complete: boolean;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface ProviderConfig {
  id?: number;
  name: string;
  provider_type: string;
  token: string;
  config: Record<string, string>;
  refresh_interval: number;
}

export type FetchStatus = 'success' | 'error' | 'never';

export interface ProviderSummary {
  id: number;
  name: string;
  provider_type: string;
  icon: string | null;
  pipeline_count: number;
  last_updated: string | null;
  refresh_interval: number;
  configured_repositories: string[];
  last_fetch_status: FetchStatus;
  last_fetch_error: string | null;
  last_fetch_at: string | null;
}

export interface TriggerParams {
  workflow_id: string;
  inputs?: Record<string, any>;
}

interface PluginCapabilities {
  pipelines: boolean;
  pipeline_runs: boolean;
  trigger: boolean;
  agents: boolean;
  artifacts: boolean;
  queues: boolean;
  custom_tables: boolean;
}

type ConfigFieldType = 'Text' | 'TextArea' | 'Password' | 'Number' | 'Select' | 'Checkbox';

export interface ConfigField {
  key: string;
  label: string;
  description: string | null;
  field_type: ConfigFieldType;
  required: boolean;
  default_value: string | null;
  options: string[] | null;
  validation_regex: string | null;
  validation_message: string | null;
}

interface ConfigSchema {
  fields: ConfigField[];
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
  | { Custom: string };

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
  | { Custom: string };

export type ColumnVisibility =
  | 'Always'
  | 'WhenPresent'
  | { WhenCapability: string }
  | { Conditional: { field: string; equals: unknown } };

export interface ColumnDefinition {
  id: string;
  label: string;
  description: string | null;
  field_path: string;
  data_type: ColumnDataType;
  renderer: CellRenderer;
  visibility: ColumnVisibility;
  default_visible: boolean;
  width: number | null;
  sortable: boolean;
  filterable: boolean;
  align: string | null;
}

export interface TableDefinition {
  id: string;
  name: string;
  description: string | null;
  columns: ColumnDefinition[];
  default_sort_column: string | null;
  default_sort_direction: string | null;
}

export interface TableSchema {
  tables: TableDefinition[];
}

export interface PluginMetadata {
  name: string;
  provider_type: string;
  version: string;
  description: string;
  author: string | null;
  icon: string | null;
  config_schema: ConfigSchema;
  table_schema: TableSchema;
  capabilities: PluginCapabilities;
  required_permissions: Permission[];
  features: Feature[];
}

export interface Permission {
  name: string;
  description: string;
  required: boolean;
}

export interface PermissionCheck {
  permission: Permission;
  granted: boolean;
}

export interface PermissionStatus {
  permissions: PermissionCheck[];
  all_granted: boolean;
  checked_at: string;
  metadata?: Record<string, string>;
}

export interface Feature {
  id: string;
  name: string;
  description: string;
  required_permissions: string[];
}

export interface FeatureAvailability {
  feature: Feature;
  available: boolean;
  missing_permissions: string[];
}

export interface ValidationResult {
  valid: boolean;
  error: string | null;
}

export interface PermissionCheckResult {
  permission_status: PermissionStatus | null;
  features: FeatureAvailability[];
}

export interface WorkflowParameter {
  name: string;
  label: string | null;
  description: string | null;
  type: 'string' | 'boolean' | 'choice' | 'number';
  default?: string | number | boolean | null;
  options?: string[];
  required: boolean;
}

export type MetricType = 'run_duration' | 'success_rate' | 'run_frequency';

export type AggregationPeriod = 'hourly' | 'daily' | 'weekly' | 'monthly';

export type AggregationType = 'avg' | 'sum' | 'min' | 'max' | 'p95' | 'p99';

export interface GlobalMetricsConfig {
  enabled: boolean;
  default_retention_days: number;
  updated_at: string;
}

export interface MetricsConfig {
  pipeline_id: string;
  enabled: boolean;
  retention_days: number;
  created_at: string;
  updated_at: string;
}

export interface MetricMetadata {
  status?: string;
  branch?: string;
  repository?: string;
  actor?: string;
}

export interface MetricEntry {
  id: number;
  pipeline_id: string;
  timestamp: string;
  metric_type: MetricType;
  value: number;
  metadata: MetricMetadata | null;
  created_at: string;
}

export interface AggregatedMetric {
  timestamp: string;
  value: number;
  count: number;
  min: number | null;
  max: number | null;
  avg: number;
}

export interface AggregatedMetrics {
  metrics: AggregatedMetric[];
  total_count: number;
  metric_type: MetricType;
  aggregation_period: AggregationPeriod;
}

export interface PipelineMetricsStats {
  pipeline_id: string;
  pipeline_name: string;
  metrics_count: number;
  oldest_metric: string | null;
  newest_metric: string | null;
}

export interface MetricsStats {
  total_metrics_count: number;
  estimated_size_bytes: number;
  estimated_size_mb: number;
  last_cleanup_at: string | null;
  updated_at: string;
  by_pipeline: PipelineMetricsStats[];
}

export type StorageBackendType = 'sqlite' | 'postgres';

export interface PostgresSettings {
  connection_string: string;
}

export interface StorageConfig {
  data_dir: string;
  backend: StorageBackendType;
  postgres?: PostgresSettings;
}

export interface GeneralConfig {
  metrics_enabled: boolean;
  default_refresh_interval: number;
}

export interface ServerConfig {
  bind_addr: string;
  cors_allow_all: boolean;
}

export interface PipedashConfig {
  general: GeneralConfig;
  server: ServerConfig;
  storage: StorageConfig;
  providers?: ProviderFileConfig[];
}

export interface ProviderFileConfig {
  name: string;
  type: string;
  token: string;
  refresh_interval?: number;
  config?: Record<string, string>;
}

export interface StorageConfigResponse {
  config: PipedashConfig;
  summary: string;
}

export type MigrationStep =
  | 'ValidateTarget'
  | 'MigrateTokens'
  | 'MigrateConfigs'
  | 'MigrateCache'
  | 'VerifyMigration'
  | 'UpdateConfig';

export interface MigrationPlan {
  from: PipedashConfig;
  to: PipedashConfig;
  steps: MigrationStep[];
  migrate_tokens: boolean;
  migrate_configs: boolean;
  migrate_cache: boolean;
  backend_changed: boolean;
  data_dir_changed: boolean;
  created_at: string;
}

export interface MigrationOptions {
  migrate_tokens: boolean;
  migrate_cache: boolean;
  token_password?: string;
  dry_run: boolean;
}

export interface MigrationResult {
  success: boolean;
  steps_completed: MigrationStep[];
  errors: string[];
  duration_ms: number;
  stats: {
    providers_migrated: number;
    tokens_migrated: number;
    cache_entries_migrated: number;
    permissions_migrated: number;
  };
  provider_id_mapping: Record<number, number>;
}

export interface ConfigIssue {
  field: string;
  message: string;
  code: string;
}

export interface ConnectionTestResult {
  success: boolean;
  message: string;
  latency_ms?: number;
}

export interface MigrationStatsPreview {
  providers_count: number;
  tokens_count: number;
  cache_entries_count: number;
}

export interface ConfigAnalysisResponse {
  valid: boolean;
  errors: ConfigIssue[];
  warnings: ConfigIssue[];
  migration_plan?: MigrationPlan;
  postgres_connection?: ConnectionTestResult;
  stats?: MigrationStatsPreview;
}

export type {
  ModalBaseProps,
  PipelineComponentProps,
} from './components'
export type {
  PipedashError,
} from './errors'
export {
  createError,
  formatErrorMessage,
  isAuthError,
  isNetworkError,
  isPermissionError,
  isPipedashError,
  isValidationError,
  toPipedashError,
} from './errors'
export type {
  DeepPartial,
  DeepReadonly,
  RetryConfig,
} from './utils'

export interface SetupStatus {
  config_exists: boolean;
  config_valid: boolean;
  validation_errors: string[];
  needs_setup: boolean;
  needs_migration: boolean;
  database_exists?: boolean;
  database_path?: string;
}

export interface ConfigContentResponse {
  content: string;
  path: string;
}

export interface StoragePathsResponse {
  config_file: string;
  pipedash_db: string;
  metrics_db: string;
  data_dir: string;
  cache_dir: string;
  vault_path: string;
}

export type PasswordSource = 'env_var' | 'keyring' | 'session' | 'none';

export interface VaultStatusResponse {
  is_unlocked: boolean;
  password_source: PasswordSource;
  backend: StorageBackendType;
  requires_password: boolean;
  is_first_time?: boolean;
}

export interface UnlockVaultResponse {
  success: boolean;
  message: string;
}
