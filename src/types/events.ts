import type { Pipeline, ProviderSummary } from './index'

export interface PipelinesUpdatedPayload {
  pipelines: Pipeline[]
  providerId?: number
  timestamp: number
}

export interface ProviderChangedPayload {
  provider: ProviderSummary
  action: 'added' | 'updated' | 'removed'
  timestamp: number
}

export interface CacheInvalidatedPayload {
  providerId?: number
  reason: 'fetch' | 'provider-change' | 'manual-refresh'
}

export interface RefreshStatusPayload {
  status: 'active' | 'idle'
  providerId?: number
}

export type EventPayloadMap = {
  'pipelines-updated': PipelinesUpdatedPayload
  'provider-added': ProviderChangedPayload
  'provider-updated': ProviderChangedPayload
  'provider-removed': ProviderChangedPayload
  'pipeline-cache-invalidated': CacheInvalidatedPayload
  'refresh-status': RefreshStatusPayload
}
