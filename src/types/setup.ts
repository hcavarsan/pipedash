import type { MigrationResult, PipedashConfig } from './storage'

export interface SetupDraft {
  backend: string
  data_dir: string
  postgres_url: string
  vault_password: string
  vault_password_confirm: string
  transfer_data: boolean
}

export interface SetupDraftResult {
  config: PipedashConfig
  errors: Record<string, string>
  password_warnings: string[]
  needs_migration: boolean
}

export interface SetupApplied {
  saved: boolean
  migration_stats: MigrationResult['stats'] | null
  restart_required: boolean
}
