import type { Pipeline, PipelineRun, ProviderHealthAlert, ProviderSummary } from './domain'

export interface PipelinesUpdatedPayload {
  pipelines: Pipeline[]
  providerId: number
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

export interface ProviderStatusUpdatedPayload {
  provider: ProviderSummary
}

export interface PipelineStatusChangedPayload {
  pipeline_ids: string[]
}

export type EventPayloadMap = {
  'pipelines-updated': PipelinesUpdatedPayload
  'pipelines-failed': { pipelines: Pipeline[] }
  'provider-added': ProviderChangedPayload
  'provider-updated': ProviderChangedPayload
  'provider-removed': ProviderChangedPayload
  'provider-status-updated': ProviderStatusUpdatedPayload
  'provider-health-alert': ProviderHealthAlert
  'providers-changed': undefined
  'pipeline-views-changed': undefined
  'pipeline-cache-invalidated': CacheInvalidatedPayload
  'pipeline-status-changed': PipelineStatusChangedPayload
  'refresh-status': RefreshStatusPayload
  'run-log-progress': { pipelineId: string; runNumber: number; nodeId: string; receivedBytes: number }
  'run-log-ready': { pipelineId: string; runNumber: number; nodeId: string; revision: number }
  'run-log-failed': { pipelineId: string; runNumber: number; nodeId: string; message: string }
  'run-log-tree-changed': { pipelineId: string; runNumber: number }
  'run-updated': { pipelineId: string; runNumber: number; run: PipelineRun }
}
