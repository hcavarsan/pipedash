import type { DataTableSortStatus } from 'mantine-datatable'
import { useMemo, useState } from 'react'

import { DEBOUNCE_DELAYS } from '../../constants/intervals'
import { PAGE_SIZES } from '../../constants/pagination'
import { useDebounce } from '../../hooks/useDebounce'
import { useIsMobile } from '../../hooks/useIsMobile'
import { type RunHistoryFilters, useRunHistoryFilters } from '../../hooks/useUrlState'
import { useRunHistory, useRunHistoryInfinite } from '../../queries/useRunHistoryQueries'
import { useTableDefinition } from '../../queries/useTableSchemaQueries'
import type { Pipeline, PipelineRun } from '../../types'
import { getDateRangeStart } from '../../utils/dateRange'
import { getValueByPath } from '../../utils/objectPath'

const EMPTY_RUNS: PipelineRun[] = []
const PAGE_SIZE = PAGE_SIZES.RUN_HISTORY

export interface RunHistoryPageData {
  filters: RunHistoryFilters
  setFilter: <K extends keyof RunHistoryFilters>(key: K, value: RunHistoryFilters[K]) => void
  selectStatus: (status: string, selected: boolean) => void
  clearFilters: () => void
  sourceRuns: PipelineRun[]
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
  hasMore: boolean
  isFetchingMore: boolean
  fetchMore: () => void
  effectiveTotalRecords: number
  pageSize: number
}

function uniqueRuns(runs: PipelineRun[]): PipelineRun[] {
  return [...new Map(runs.map((run) => [run.run_number, run])).values()]
}

function compareRuns(a: PipelineRun, b: PipelineRun, sort: DataTableSortStatus<PipelineRun>): number {
  const accessor = String(sort.columnAccessor)
  const isDate = accessor === 'started_at' || accessor === 'concluded_at'
  const read = (run: PipelineRun) => {
    const value = getValueByPath(run, accessor)

    if (isDate) {
      return typeof value === 'string' || typeof value === 'number' ? new Date(value).getTime() : 0
    }

    return value ?? ''
  }
  const left = read(a)
  const right = read(b)

  if (typeof left === 'string' && typeof right === 'string') {
    return sort.direction === 'asc' ? left.localeCompare(right) : right.localeCompare(left)
  }

  if (typeof left === 'number' && typeof right === 'number') {
    return sort.direction === 'asc' ? left - right : right - left
  }

  return 0
}

export function useRunHistoryPageData(pipeline: Pipeline | null): RunHistoryPageData {
  const { isMobile } = useIsMobile()
  const pipelineId = pipeline?.id ?? ''
  const { filters, setFilter, selectStatus, clearFilters } = useRunHistoryFilters()
  const { search, status: statusFilter, branch: branchFilter, actor: actorFilter, dateRange, page } = filters
  const debouncedSearch = useDebounce(search, DEBOUNCE_DELAYS.FILTER)

  const { data: runsTable } = useTableDefinition(pipeline?.provider_id ?? 0, 'pipeline_runs')
  const [sortOverride, setSortOverride] = useState<DataTableSortStatus<PipelineRun> | null>(null)
  const sortStatus: DataTableSortStatus<PipelineRun> = sortOverride ?? {
    columnAccessor: runsTable?.default_sort_column ?? 'run_number',
    direction: runsTable?.default_sort_direction === 'asc' ? 'asc' : 'desc',
  }

  const paged = useRunHistory(pipelineId, page, PAGE_SIZE, !isMobile)
  const infinite = useRunHistoryInfinite(isMobile ? pipelineId : '', PAGE_SIZE)
  const lastPage = isMobile ? infinite.data?.pages.at(-1) : paged.data

  const sourceRuns = useMemo(
    () =>
      isMobile
        ? uniqueRuns(infinite.data?.pages.flatMap((entry) => entry.runs) ?? [])
        : (paged.data?.runs ?? EMPTY_RUNS),
    [isMobile, infinite.data, paged.data]
  )

  const branches = useMemo(
    () => Array.from(new Set(sourceRuns.map((run) => run.branch).filter((branch): branch is string => !!branch))),
    [sourceRuns]
  )
  const actors = useMemo(
    () => Array.from(new Set(sourceRuns.map((run) => run.actor).filter((actor): actor is string => !!actor))),
    [sourceRuns]
  )

  const runs = useMemo(() => {
    const rangeStart = getDateRangeStart(dateRange)
    const term = debouncedSearch.toLowerCase()

    return sourceRuns
      .filter((run) => {
        if (rangeStart && !(run.started_at && new Date(run.started_at) >= rangeStart)) {
          return false
        }

        if (
          term &&
          ![run.branch, run.commit_sha, run.commit_message, run.actor].some((field) =>
            field?.toLowerCase().includes(term)
          )
        ) {
          return false
        }

        return (
          (statusFilter.length === 0 || statusFilter.includes(run.status)) &&
          (!branchFilter || run.branch === branchFilter) &&
          (!actorFilter || run.actor === actorFilter)
        )
      })
      .sort((a, b) => compareRuns(a, b, sortStatus))
  }, [sourceRuns, dateRange, debouncedSearch, statusFilter, branchFilter, actorFilter, sortStatus])

  const totalCount = lastPage?.total_count ?? 0
  const hasMore = lastPage?.has_more ?? false
  const effectiveTotalRecords = paged.data
    ? hasMore
      ? Math.max((page + 4) * PAGE_SIZE, totalCount)
      : Math.max(totalCount, PAGE_SIZE)
    : Math.max(page * PAGE_SIZE, PAGE_SIZE)
  const active = isMobile ? infinite : paged

  return {
    filters,
    setFilter,
    selectStatus,
    clearFilters,
    sourceRuns,
    runs,
    branches,
    actors,
    sortStatus,
    setSortOverride,
    isLoading: active.isLoading,
    isFetching: active.isFetching,
    isError: active.isError,
    error: active.error,
    refetch: () => active.refetch(),
    totalCount,
    isComplete: lastPage?.is_complete ?? false,
    hasMore,
    isFetchingMore: infinite.isFetchingNextPage,
    fetchMore: () => {
      if (isMobile && infinite.hasNextPage && !infinite.isFetchingNextPage) {
        void infinite.fetchNextPage()
      }
    },
    effectiveTotalRecords,
    pageSize: PAGE_SIZE,
  }
}
