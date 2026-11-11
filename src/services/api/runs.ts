import { REFRESH_TIMEOUT_MS } from '../../constants/timeouts'
import type { RefreshResult } from '../../types/health'
import type { Pipeline, PipelineRun, TriggerDraft, TriggerResult } from '../../types/pipelines'
import type { CancellationBatch, CancellationTargets } from '../../types/run_action_batch'
import type { RunHistoryPage } from '../../types/run_history'
import type { HttpClient, RequestOptions } from '../http'
import type { RunsService } from '../types'

const REFRESH_REQUEST: RequestOptions = { timeoutMs: REFRESH_TIMEOUT_MS }

const runPath = (pipelineId: string, runNumber: number): string =>
  `/pipelines/${encodeURIComponent(pipelineId)}/runs/${runNumber}`

export function createRunsApi(client: HttpClient): RunsService {
  return {
    getCachedPipelines: async (providerId) => {
      const params = providerId ? `?provider_id=${providerId}` : ''
      const result = await client.get<Pipeline[]>(`/pipelines/cached${params}`)

      return result || []
    },

    fetchRunHistory: async (pipelineId, query) =>
      client.post<RunHistoryPage>(`/pipelines/${encodeURIComponent(pipelineId)}/runs/query`, query),

    preparePipelineTrigger: async (pipelineId, rerunOf, inputs) =>
      client.post<TriggerDraft>(`/pipelines/${encodeURIComponent(pipelineId)}/trigger/prepare`, {
        rerun_of: rerunOf,
        inputs,
      }),

    triggerPipeline: async (params) =>
      client.post<TriggerResult>(`/pipelines/${encodeURIComponent(params.workflow_id)}/trigger`, {
        workflow_id: params.workflow_id,
        inputs: params.inputs,
      }),

    getCancellationTargets: async (pipelineIds) =>
      client.post<CancellationTargets>('/pipelines/cancellation-targets', { pipeline_ids: pipelineIds }),

    cancelPipelineRuns: async (targets) => client.post<CancellationBatch>('/pipelines/cancel-runs', { targets }),

    refreshProvider: async (id) => client.post<RefreshResult>(`/providers/${id}/refresh`, undefined, REFRESH_REQUEST),

    refreshAll: async () => client.post<RefreshResult[]>('/refresh/all', undefined, REFRESH_REQUEST),

    updateGeneralSettings: async (update) => {
      await client.requestVoid('PUT', '/settings/general', update)
    },

    getWorkflowRunDetails: async (pipelineId, runNumber) => client.get<PipelineRun>(runPath(pipelineId, runNumber)),

    cancelPipelineRun: async (pipelineId, runNumber) => {
      await client.requestVoid('POST', `${runPath(pipelineId, runNumber)}/cancel`)
    },

    forceCancelPipelineRun: async (pipelineId, runNumber) => {
      await client.requestVoid('POST', `${runPath(pipelineId, runNumber)}/force-cancel`)
    },

    reviewPipelineRun: async (pipelineId, runNumber, decision) => {
      await client.requestVoid('POST', `${runPath(pipelineId, runNumber)}/review`, { decision })
    },

    rerunFailedJobs: async (pipelineId, runNumber) => {
      await client.requestVoid('POST', `${runPath(pipelineId, runNumber)}/rerun-failed-jobs`)
    },

    rerunRun: async (pipelineId, runNumber) => client.post<TriggerResult>(`${runPath(pipelineId, runNumber)}/rerun`),

    enablePipeline: async (pipelineId) => {
      await client.requestVoid('POST', `/pipelines/${encodeURIComponent(pipelineId)}/enable`)
    },
  }
}
