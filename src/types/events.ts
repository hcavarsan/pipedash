import type { ShownNotification } from './notifications'
import type { Pipeline, PipelineRun } from './pipelines'
import type { ProviderSummary } from './providers'

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

export interface RunHistoryCacheInvalidatedPayload {
  pipeline_id?: string
  run_number?: number
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
  notification: ShownNotification
  'provider-added': ProviderChangedPayload
  'provider-updated': ProviderChangedPayload
  'provider-removed': ProviderChangedPayload
  'provider-status-updated': ProviderStatusUpdatedPayload
  'alert-recorded': { id: string; seq: number }
  'providers-changed': undefined
  'pipeline-views-changed': undefined
  'pipeline-status-changed': PipelineStatusChangedPayload
  'run-history-cache-invalidated': RunHistoryCacheInvalidatedPayload
  'metrics-global-config-changed': undefined
  'refresh-status': RefreshStatusPayload
  'run-log-progress': { pipelineId: string; runNumber: number; nodeId: string; receivedBytes: number }
  'run-log-ready': { pipelineId: string; runNumber: number; nodeId: string; revision: number }
  'run-log-failed': { pipelineId: string; runNumber: number; nodeId: string; message: string }
  'run-log-tree-changed': { pipelineId: string; runNumber: number }
  'run-updated': { pipelineId: string; runNumber: number; run: PipelineRun }
}
