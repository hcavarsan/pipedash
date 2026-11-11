import type { FailureKind, FetchStatus, HealthSignal, Maintenance, ProviderHealth } from './health'

export interface ProviderConfig {
  id?: number
  name: string
  provider_type: string
  token: string
  config: Record<string, string>
  refresh_interval: number
}

export type ProviderRecovery = 'sign_in' | 'settings'

export type AttentionTone = 'warning' | 'danger'

export type ProviderOpenAction = 'pipelines' | 'status' | 'sign_in'

export type AttentionSchedule = { type: 'checking_now' } | { type: 'next_attempt'; at: string }

export interface ProviderFreshness {
  state: ProviderTone
  stale_at: string | null
}

export interface ProviderAttention {
  tone: AttentionTone | null
  headline: string | null
  detail: string | null
  hint: string | null
  fetch_hint: string | null
  schedule: AttentionSchedule | null
  kind: FailureKind | null
  recovery: ProviderRecovery | null
  episode_key: string | null
  refreshing_normally: boolean
  stale_signals: HealthSignal[]
  scheduled_maintenances: Maintenance[]
  open_action: ProviderOpenAction
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
  last_success_at: string | null
  paused_until: string | null
  freshness: ProviderFreshness
  recovery: ProviderRecovery | null
  health: ProviderHealth
  attention: ProviderAttention
}

export interface SignInStart {
  id: string
  authorize_url: string
  label: string
  manual: boolean
}

export type SignInStatus =
  | { state: 'pending' }
  | { state: 'completed'; account: string | null }
  | { state: 'failed'; message: string }
  | { state: 'expired' }

export interface ProviderImportFailure {
  name: string
  error: string
}

export interface ProviderImportResult {
  added: string[]
  skipped: string[]
  failed: ProviderImportFailure[]
  not_attempted: string[]
}

export type ProviderTone = 'fresh' | 'stale' | 'down' | 'never'
