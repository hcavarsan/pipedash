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
  token_password?: string | undefined
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
