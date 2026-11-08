import { invoke } from '@tauri-apps/api/core'

import { RUN_LOG_SEARCH_TIMEOUT_MS } from '../../constants/timeouts'
import type { LogLinesPage, LogSearchResult, RunLogDocument, RunLogTree } from '../../types/logs'
import type { ExportFileResult } from '../../types/native'
import { isRemoteClient } from '../connection'
import { API_BASE, type HttpClient } from '../http'
import { promptLogDestination } from '../tauri/platform'
import type { LogsService } from '../types'

const runLogPath = (pipelineId: string, runNumber: number, nodeId?: string): string => {
  const base = `/pipelines/${encodeURIComponent(pipelineId)}/runs/${runNumber}/logs`

  return nodeId === undefined ? base : `${base}/${encodeURIComponent(nodeId)}`
}

export function createLogsApi(client: HttpClient): LogsService {
  return {
    getCacheStats: async () => client.get('/cache/stats'),

    clearPipelinesCache: async () => {
      const result = await client.delete<{ cleared: number }>('/cache/pipelines')

      return result.cleared
    },

    clearAllRunHistoryCaches: async () => {
      await client.requestVoid('DELETE', '/cache/run-history')
    },

    clearWorkflowParamsCache: async () => {
      await client.requestVoid('DELETE', '/cache/workflow-params')
    },

    clearAllCaches: async () => {
      await client.requestVoid('DELETE', '/cache')
    },

    getRunLogTree: async (pipelineId, runNumber) => client.get<RunLogTree>(runLogPath(pipelineId, runNumber)),

    openRunLog: async (pipelineId, runNumber, nodeId, retry) =>
      client.get<RunLogDocument>(`${runLogPath(pipelineId, runNumber, nodeId)}?retry=${retry}`),

    readRunLogLines: async (pipelineId, runNumber, nodeId, revision, start, count, maxLineUnits) =>
      client.get<LogLinesPage>(
        `${runLogPath(pipelineId, runNumber, nodeId)}/lines?revision=${revision}&start=${start}&count=${count}&max_line_units=${maxLineUnits}`
      ),

    searchRunLog: async (pipelineId, runNumber, nodeId, revision, sessionId, query, signal) =>
      client.request<LogSearchResult>(
        'POST',
        `${runLogPath(pipelineId, runNumber, nodeId)}/search?revision=${revision}&session=${encodeURIComponent(sessionId)}`,
        query,
        { timeoutMs: RUN_LOG_SEARCH_TIMEOUT_MS, signal }
      ),

    clearRunLogsCache: async () => {
      await client.requestVoid('DELETE', '/cache/run-logs')
    },

    exportRunLog: async (pipelineId, runNumber, nodeId, revision, fileName) => {
      const remote = isRemoteClient()
      const destination = remote ? await promptLogDestination(fileName) : null

      if (destination === undefined) {
        return false
      }

      const { ticket } = await client.post<{ ticket: string }>(
        `${runLogPath(pipelineId, runNumber, nodeId)}/download-ticket`,
        {
          revision,
          file_name: fileName,
        }
      )

      if (remote) {
        return (await invoke<ExportFileResult>('remote_save_download', { ticket, fileName, destination })).shared
      }

      const link = document.createElement('a')

      link.href = `${API_BASE}/downloads/${ticket}`
      link.download = fileName
      link.click()

      return true
    },
  }
}
