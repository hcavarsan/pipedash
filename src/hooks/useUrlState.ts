import { useCallback, useLayoutEffect, useMemo, useRef } from 'react'
import { useLocation, useSearchParams } from 'react-router-dom'

import { STATUS_META } from '../theme/status'
import type { PipelineStatus, PipelineViewQuery } from '../types'
import { isDateRange } from '../utils/dateRange'

const parseStatuses = (value: string | null): PipelineStatus[] =>
  (value ?? '').split(',').filter((status): status is PipelineStatus => Object.hasOwn(STATUS_META, status))

const setParam = (params: URLSearchParams, key: string, value: string | string[] | number | null) => {
  const text = Array.isArray(value) ? value.join(',') : value

  if (text === null || text === '') {
    params.delete(key)
  } else {
    params.set(key, String(text))
  }
}

const selectStatusIn = (params: URLSearchParams, status: string, selected: boolean) => {
  const current: string[] = parseStatuses(params.get('status'))

  if (current.includes(status) !== selected) {
    setParam(params, 'status', selected ? [...current, status] : current.filter((item) => item !== status))
  }
}

const PIPELINE_FILTER_KEYS = ['search', 'status', 'provider', 'organization', 'dateRange', 'hidden']

const METRICS_FILTER_KEYS = ['dateRange', 'period']

const parsePage = (value: string | null): number => {
  const page = Number(value)

  return Number.isSafeInteger(page) && page >= 1 ? page : 1
}

const RUN_FILTER_KEYS = ['search', 'status', 'branch', 'actor', 'dateRange', 'page']

function useFilterParams(keys: readonly string[]) {
  const [searchParams, setSearchParams] = useSearchParams()
  const { state } = useLocation()
  const latest = useRef(searchParams)
  const pending = useRef<string[]>([])

  useLayoutEffect(() => {
    const written = pending.current.indexOf(searchParams.toString())

    pending.current = written === -1 ? [] : pending.current.slice(written + 1)

    if (pending.current.length === 0) {
      latest.current = searchParams
    }
  }, [searchParams])

  const update = useCallback(
    (change: (params: URLSearchParams) => void) => {
      const next = new URLSearchParams(latest.current)

      change(next)

      if (next.toString() === latest.current.toString()) {
        return
      }

      latest.current = next
      pending.current.push(next.toString())
      setSearchParams(next, { replace: true, state })
    },
    [setSearchParams, state]
  )

  const clearFilters = useCallback(
    () =>
      update((params) => {
        for (const key of keys) {
          params.delete(key)
        }
      }),
    [update, keys]
  )
  const selectStatus = useCallback(
    (status: string, selected: boolean) => update((params) => selectStatusIn(params, status, selected)),
    [update]
  )

  return { searchParams, update, clearFilters, selectStatus }
}

export interface PipelineFilters {
  search: string
  status: string[]
  provider: string | null
  organization: string | null
  dateRange: string | null
  hidden: string | null
}

export function usePipelineFilters(): {
  filters: PipelineFilters
  setFilter: <K extends keyof PipelineFilters>(key: K, value: PipelineFilters[K]) => void
  selectStatus: (status: string, selected: boolean) => void
  clearFilters: () => void
} {
  const { searchParams, update, clearFilters, selectStatus } = useFilterParams(PIPELINE_FILTER_KEYS)

  const filters = useMemo(
    (): PipelineFilters => ({
      search: searchParams.get('search') ?? '',
      status: parseStatuses(searchParams.get('status')),
      provider: searchParams.get('provider'),
      organization: searchParams.get('organization'),
      dateRange: searchParams.get('dateRange'),
      hidden: searchParams.get('hidden'),
    }),
    [searchParams]
  )

  const setFilter = useCallback(
    <K extends keyof PipelineFilters>(key: K, value: PipelineFilters[K]) =>
      update((params) => setParam(params, key, value)),
    [update]
  )

  return { filters, setFilter, selectStatus, clearFilters }
}

export function usePipelineViewQuery(): PipelineViewQuery {
  const [searchParams] = useSearchParams()

  return useMemo((): PipelineViewQuery => {
    const provider = Number(searchParams.get('provider'))
    const statuses = parseStatuses(searchParams.get('status'))
    const dateRange = searchParams.get('dateRange')

    return {
      view: searchParams.get('view') ?? 'all',
      provider_id: searchParams.get('provider') && Number.isFinite(provider) ? provider : undefined,
      search: searchParams.get('search') || undefined,
      statuses: statuses.length > 0 ? statuses : undefined,
      organization: searchParams.get('organization') || undefined,
      date_range: isDateRange(dateRange) ? dateRange : undefined,
      show_hidden: searchParams.get('hidden') === 'show',
      utc_offset_minutes: -new Date().getTimezoneOffset(),
    }
  }, [searchParams])
}

export interface RunHistoryFilters {
  search: string
  status: string[]
  branch: string | null
  actor: string | null
  dateRange: string | null
  page: number
}

export function useRunHistoryFilters(): {
  filters: RunHistoryFilters
  setFilter: <K extends keyof RunHistoryFilters>(key: K, value: RunHistoryFilters[K]) => void
  selectStatus: (status: string, selected: boolean) => void
  clearFilters: () => void
} {
  const { searchParams, update, clearFilters, selectStatus } = useFilterParams(RUN_FILTER_KEYS)

  const filters = useMemo(
    (): RunHistoryFilters => ({
      search: searchParams.get('search') ?? '',
      status: parseStatuses(searchParams.get('status')),
      branch: searchParams.get('branch'),
      actor: searchParams.get('actor'),
      dateRange: searchParams.get('dateRange'),
      page: parsePage(searchParams.get('page')),
    }),
    [searchParams]
  )

  const setFilter = useCallback(
    <K extends keyof RunHistoryFilters>(key: K, value: RunHistoryFilters[K]) =>
      update((params) => setParam(params, key, key === 'page' && value === 1 ? null : value)),
    [update]
  )

  return { filters, setFilter, selectStatus, clearFilters }
}

export interface MetricsFilters {
  dateRange: string
  period: string
}

export function useMetricsFilters(): {
  filters: MetricsFilters
  setFilter: <K extends keyof MetricsFilters>(key: K, value: MetricsFilters[K]) => void
} {
  const { searchParams, update } = useFilterParams(METRICS_FILTER_KEYS)

  const filters = useMemo(
    (): MetricsFilters => ({
      dateRange: searchParams.get('dateRange') ?? '24h',
      period: searchParams.get('period') ?? 'hourly',
    }),
    [searchParams]
  )

  const setFilter = useCallback(
    <K extends keyof MetricsFilters>(key: K, value: MetricsFilters[K]) =>
      update((params) => {
        const isDefault = (key === 'dateRange' && value === '24h') || (key === 'period' && value === 'hourly')

        setParam(params, key, isDefault ? null : value)
      }),
    [update]
  )

  return { filters, setFilter }
}
