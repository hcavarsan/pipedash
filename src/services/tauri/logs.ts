import { invoke } from '@tauri-apps/api/core'

import { RUN_LOG_SEARCH_TIMEOUT_MS } from '../../constants/timeouts'
import type { LogLinesPage, LogSearchResult, RunLogDocument, RunLogTree } from '../../types/logs'
import type { ExportFileResult } from '../../types/native'
import { invokeWithTimeout } from '../invoke'
import type { LogsService } from '../types'
import { promptLogDestination } from './platform'

export const logsTauriService: LogsService = {
  getCacheStats: async () => invoke('get_cache_stats'),

  clearPipelinesCache: async () => invoke<number>('clear_pipelines_cache'),

  clearAllRunHistoryCaches: async () => invoke<void>('clear_all_run_history_caches'),

  clearWorkflowParamsCache: async () => invoke<void>('clear_workflow_params_cache'),

  clearAllCaches: async () => invoke<void>('clear_all_caches'),

  getRunLogTree: async (pipelineId, runNumber) => invoke<RunLogTree>('get_run_log_tree', { pipelineId, runNumber }),

  openRunLog: async (pipelineId, runNumber, nodeId, retry) =>
    invoke<RunLogDocument>('open_run_log', { pipelineId, runNumber, nodeId, retry }),

  readRunLogLines: async (pipelineId, runNumber, nodeId, revision, start, count, maxLineUnits) =>
    invoke<LogLinesPage>('read_run_log_lines', {
      pipelineId,
      runNumber,
      nodeId,
      query: { revision, start, count, max_line_units: maxLineUnits },
    }),

  searchRunLog: async (pipelineId, runNumber, nodeId, revision, sessionId, query, signal) => {
    signal?.throwIfAborted()

    return invokeWithTimeout<LogSearchResult>(
      'search_run_log',
      { pipelineId, runNumber, nodeId, revision, sessionId, query },
      RUN_LOG_SEARCH_TIMEOUT_MS,
      false
    )
  },

  clearRunLogsCache: async () => invoke<void>('clear_run_logs_cache'),

  exportRunLog: async (pipelineId, runNumber, nodeId, revision, fileName) => {
    const destination = await promptLogDestination(fileName)

    if (destination === undefined) {
      return false
    }

    const args = { pipelineId, runNumber, nodeId, revision, fileName, destination }

    return (await invoke<ExportFileResult>('export_run_log', { args })).shared
  },
}
