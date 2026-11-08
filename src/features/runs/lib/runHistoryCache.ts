import type { InfiniteData, QueryClient } from '@tanstack/react-query'

import { queryKeys } from '../../../lib/queryKeys'
import type { PipelineRun } from '../../../types/pipelines'
import type { RunHistoryPage } from '../../../types/run_history'

type CachedHistory = RunHistoryPage | InfiniteData<RunHistoryPage, number>

const withRun = (page: RunHistoryPage, run: PipelineRun): RunHistoryPage | undefined =>
  page.runs.some((entry) => entry.run_number === run.run_number)
    ? { ...page, runs: page.runs.map((entry) => (entry.run_number === run.run_number ? run : entry)) }
    : undefined

export function updateRunInHistory(queryClient: QueryClient, run: PipelineRun): void {
  queryClient.setQueriesData<CachedHistory>({ queryKey: queryKeys.runs.list(run.pipeline_id) }, (cached) => {
    if (!cached) {
      return undefined
    }

    if (!('pages' in cached)) {
      return withRun(cached, run)
    }

    const pages = cached.pages.map((page) => withRun(page, run) ?? page)

    return pages.some((page, index) => page !== cached.pages[index]) ? { ...cached, pages } : undefined
  })
}

export function invalidateRunHistory(queryClient: QueryClient, pipelineId: string, runNumber: number): void {
  void queryClient.invalidateQueries({ queryKey: queryKeys.runs.detail(pipelineId, runNumber) })
  void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })
}
