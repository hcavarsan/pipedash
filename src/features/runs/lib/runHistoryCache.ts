import type { InfiniteData, QueryClient } from '@tanstack/react-query'

import { queryKeys } from '../../../lib/queryKeys'
import type { PaginatedRunHistory, PipelineRun } from '../../../types/domain'

type CachedHistory = PaginatedRunHistory | InfiniteData<PaginatedRunHistory, number>

const withRun = (page: PaginatedRunHistory, run: PipelineRun): PaginatedRunHistory | undefined =>
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
