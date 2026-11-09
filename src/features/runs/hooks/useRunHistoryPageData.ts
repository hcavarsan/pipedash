import type { DataTableSortStatus } from 'mantine-datatable'
import { useMemo, useState } from 'react'

import { DEBOUNCE_DELAYS } from '../../../constants/intervals'
import { PAGE_SIZES } from '../../../constants/pagination'
import { useDebounce } from '../../../hooks/useDebounce'
import { useIsMobile } from '../../../hooks/useIsMobile'
import { type RunHistoryFilters, useRunHistoryFilters } from '../../../hooks/useUrlState'
import { runHistoryQuery } from '../../../lib/runHistoryQuery'
import { useRunHistory, useRunHistoryInfinite } from '../../../queries/useRunHistoryQueries'
import { STATUS_META } from '../../../theme/status'
import type { Pipeline, PipelineRun, PipelineStatus } from '../../../types/pipelines'
import type { RunHistoryScanNotice } from '../../../types/run_history'
import { isDateRange } from '../../../utils/dateRange'
import { useTableDefinition } from '../queries/useTableSchemaQueries'

const isPipelineStatus = (value: string): value is PipelineStatus => value in STATUS_META

const EMPTY_RUNS: PipelineRun[] = []
const PAGE_SIZE = PAGE_SIZES.RUN_HISTORY

export interface RunHistoryPageData {
  filters: RunHistoryFilters
  setFilter: <K extends keyof RunHistoryFilters>(key: K, value: RunHistoryFilters[K]) => void
  selectStatus: (status: string, selected: boolean) => void
  clearFilters: () => void
  filtered: boolean
  runs: PipelineRun[]
  branches: string[]
  actors: string[]
  sortStatus: DataTableSortStatus<PipelineRun>
  setSortOverride: (status: DataTableSortStatus<PipelineRun>) => void
  isLoading: boolean
  isFetching: boolean
  isError: boolean
  error: Error | null
  refetch: () => Promise<unknown>
  totalCount: number
  isComplete: boolean
  scanNotice: RunHistoryScanNotice | null
  hasMore: boolean
  isFetchingMore: boolean
  fetchMore: () => void
  pageSize: number
}

export function useRunHistoryPageData(pipeline: Pipeline | null): RunHistoryPageData {
  const { isMobile } = useIsMobile()
  const pipelineId = pipeline?.id ?? ''
  const { filters, setFilter, selectStatus, clearFilters } = useRunHistoryFilters()
  const { search, status, branch, actor, dateRange, page } = filters
  const debouncedSearch = useDebounce(search, DEBOUNCE_DELAYS.FILTER)

  const { data: runsTable } = useTableDefinition(pipeline?.provider_id ?? 0, 'pipeline_runs')
  const [sortOverride, setSortOverride] = useState<DataTableSortStatus<PipelineRun> | null>(null)
  const sortStatus: DataTableSortStatus<PipelineRun> = sortOverride ?? {
    columnAccessor: runsTable?.default_sort_column ?? 'run_number',
    direction: runsTable?.default_sort_direction === 'asc' ? 'asc' : 'desc',
  }

  const scanKey = pipelineId
  const [depth, setDepth] = useState({ key: scanKey, value: 1 })
  const scanDepth = depth.key === scanKey ? depth.value : 1

  const query = useMemo(
    () =>
      runHistoryQuery(page, PAGE_SIZE, {
        search: debouncedSearch || undefined,
        statuses: status.filter(isPipelineStatus),
        branch: branch ?? undefined,
        actor: actor ?? undefined,
        date_range: isDateRange(dateRange) ? dateRange : undefined,
        sort_column: String(sortStatus.columnAccessor),
        direction: sortStatus.direction,
        scan_depth: scanDepth,
      }),
    [page, debouncedSearch, status, branch, actor, dateRange, sortStatus, scanDepth]
  )
  const { page: _page, scan_depth: _depth, ...infiniteQuery } = query

  const paged = useRunHistory(pipelineId, query, !isMobile)
  const infinite = useRunHistoryInfinite(isMobile ? pipelineId : '', infiniteQuery)
  const lastPage = isMobile ? infinite.data?.pages.at(-1) : paged.data
  const active = isMobile ? infinite : paged
  const runs = useMemo(
    () =>
      isMobile
        ? [
            ...new Map(
              (infinite.data?.pages.flatMap((entry) => entry.runs) ?? []).map((run) => [run.run_number, run])
            ).values(),
          ]
        : (paged.data?.runs ?? EMPTY_RUNS),
    [isMobile, infinite.data, paged.data]
  )

  return {
    filters,
    setFilter,
    selectStatus,
    clearFilters,
    filtered: !!(search || status.length > 0 || branch || actor || isDateRange(dateRange)),
    runs,
    branches: lastPage?.branches ?? [],
    actors: lastPage?.actors ?? [],
    sortStatus,
    setSortOverride,
    isLoading: active.isLoading,
    isFetching: active.isFetching,
    isError: active.isError,
    error: active.error,
    refetch: () => active.refetch(),
    totalCount: lastPage?.total_count ?? 0,
    isComplete: lastPage?.is_complete ?? false,
    scanNotice: lastPage?.scan_notice ?? null,
    hasMore: isMobile ? infinite.hasNextPage : (lastPage?.can_scan_deeper ?? false),
    isFetchingMore: isMobile ? infinite.isFetchingNextPage : paged.isFetching,
    fetchMore: () => {
      if (!isMobile) {
        setDepth({ key: scanKey, value: scanDepth + 1 })
        return
      }
      if (infinite.hasNextPage && !infinite.isFetchingNextPage) {
        void infinite.fetchNextPage()
      }
    },
    pageSize: PAGE_SIZE,
  }
}
