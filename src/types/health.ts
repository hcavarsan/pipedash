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
