import { invoke } from '@tauri-apps/api/core'

import { REFRESH_TIMEOUT_MS } from '../../constants/timeouts'
import { logger } from '../../lib/logger'
import type { RefreshResult } from '../../types/health'
import type { Pipeline, PipelineRun, TriggerDraft, TriggerResult } from '../../types/pipelines'
import type { CancellationBatch, CancellationTargets } from '../../types/run_action_batch'
import type { RunHistoryPage } from '../../types/run_history'
import { invokeWithTimeout } from '../invoke'
import type { RunsService } from '../types'

export const runsTauriService: RunsService = {
  getCachedPipelines: async (providerId) => {
    logger.debug('tauriService', 'Invoking get_cached_pipelines...', { providerId })

    try {
      const result = await invokeWithTimeout<Pipeline[]>('get_cached_pipelines', {
        providerId: providerId ?? null,
      })

      logger.debug('tauriService', 'get_cached_pipelines returned', { count: result?.length ?? 0, unit: 'items' })

      return result
    } catch (err) {
      logger.error('tauriService', 'get_cached_pipelines error', err)
      throw err
    }
  },

  fetchRunHistory: async (pipelineId, query) => invoke<RunHistoryPage>('fetch_run_history', { pipelineId, query }),

  preparePipelineTrigger: async (pipelineId, rerunOf, inputs) =>
    invoke<TriggerDraft>('prepare_pipeline_trigger', { pipelineId, rerunOf, inputs }),

  triggerPipeline: async (params) => invoke<TriggerResult>('trigger_pipeline', { params }),

  getCancellationTargets: async (pipelineIds) =>
    invoke<CancellationTargets>('get_cancellation_targets', { pipelineIds }),

  cancelPipelineRuns: async (targets) => invoke<CancellationBatch>('cancel_pipeline_runs', { targets }),

  refreshProvider: async (id) => invokeWithTimeout<RefreshResult>('refresh_provider', { id }, REFRESH_TIMEOUT_MS),

  refreshAll: async () => invokeWithTimeout<RefreshResult[]>('refresh_all', undefined, REFRESH_TIMEOUT_MS),

  updateGeneralSettings: async (update) => invokeWithTimeout<void>('update_general_settings', { update }),

  getWorkflowRunDetails: async (pipelineId, runNumber) =>
    invoke<PipelineRun>('get_workflow_run_details', { pipelineId, runNumber }),

  cancelPipelineRun: async (pipelineId, runNumber) => invoke<void>('cancel_pipeline_run', { pipelineId, runNumber }),

  forceCancelPipelineRun: async (pipelineId, runNumber) =>
    invoke<void>('force_cancel_pipeline_run', { pipelineId, runNumber }),

  reviewPipelineRun: async (pipelineId, runNumber, decision) =>
    invoke<void>('review_pipeline_run', { pipelineId, runNumber, decision }),

  rerunFailedJobs: async (pipelineId, runNumber) => invoke<void>('rerun_failed_jobs', { pipelineId, runNumber }),

  rerunRun: async (pipelineId, runNumber) => invoke<TriggerResult>('rerun_pipeline_run', { pipelineId, runNumber }),

  enablePipeline: async (pipelineId) => invoke<void>('enable_pipeline', { pipelineId }),
}
