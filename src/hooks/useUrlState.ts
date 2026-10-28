import { useCallback, useMemo } from 'react'
import { useLocation, useSearchParams } from 'react-router-dom'

import { STATUS_META } from '../theme/status'
import type { PipelineStatus, PipelineViewQuery } from '../types'
import { isDateRange } from '../utils/dateRange'

const parseStatuses = (value: string | null): PipelineStatus[] =>
  (value ?? '').split(',').filter((status): status is PipelineStatus => status in STATUS_META)

const latestParams = () => new URLSearchParams(window.location.search)

export const toggledStatuses = (status: string) => {
  const current = parseStatuses(latestParams().get('status'))

  return current.includes(status as PipelineStatus) ? current.filter((item) => item !== status) : [...current, status]
}

const setParam = (params: URLSearchParams, key: string, value: string | string[] | number | null) => {
  const text = Array.isArray(value) ? value.join(',') : value

  if (text === null || text === '') {
    params.delete(key)
  } else {
    params.set(key, String(text))
  }
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
  clearFilters: () => void
} {
  const [searchParams, setSearchParams] = useSearchParams()
  const { state } = useLocation()

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
    <K extends keyof PipelineFilters>(key: K, value: PipelineFilters[K]) => {
      setSearchParams(
        () => {
          const next = latestParams()

          setParam(next, key, value)

          return next
        },
        { replace: true, state }
      )
    },
    [setSearchParams, state]
  )

  const clearFilters = useCallback(() => {
    setSearchParams(
      () => {
        const next = latestParams()

        next.delete('search')
        next.delete('status')
        next.delete('provider')
        next.delete('organization')
        next.delete('dateRange')
        next.delete('hidden')

        return next
      },
      { replace: true, state }
    )
  }, [setSearchParams, state])

  return { filters, setFilter, clearFilters }
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
  clearFilters: () => void
} {
  const [searchParams, setSearchParams] = useSearchParams()
  const { state } = useLocation()

  const filters = useMemo(
    (): RunHistoryFilters => ({
      search: searchParams.get('search') ?? '',
      status: parseStatuses(searchParams.get('status')),
      branch: searchParams.get('branch'),
      actor: searchParams.get('actor'),
      dateRange: searchParams.get('dateRange'),
      page: Number(searchParams.get('page')) || 1,
    }),
    [searchParams]
  )

  const setFilter = useCallback(
    <K extends keyof RunHistoryFilters>(key: K, value: RunHistoryFilters[K]) => {
      setSearchParams(
        () => {
          const next = latestParams()

          setParam(next, key, key === 'page' && value === 1 ? null : value)

          return next
        },
        { replace: true, state }
      )
    },
    [setSearchParams, state]
  )

  const clearFilters = useCallback(() => {
    setSearchParams(
      () => {
        const next = latestParams()

        next.delete('search')
        next.delete('status')
        next.delete('branch')
        next.delete('actor')
        next.delete('dateRange')
        next.delete('page')

        return next
      },
      { replace: true, state }
    )
  }, [setSearchParams, state])

  return { filters, setFilter, clearFilters }
}

export interface MetricsFilters {
  dateRange: string
  period: string
}

export function useMetricsFilters(): {
  filters: MetricsFilters
  setFilter: <K extends keyof MetricsFilters>(key: K, value: MetricsFilters[K]) => void
} {
  const [searchParams, setSearchParams] = useSearchParams()
  const { state } = useLocation()

  const filters = useMemo(
    (): MetricsFilters => ({
      dateRange: searchParams.get('dateRange') ?? '24h',
      period: searchParams.get('period') ?? 'hourly',
    }),
    [searchParams]
  )

  const setFilter = useCallback(
    <K extends keyof MetricsFilters>(key: K, value: MetricsFilters[K]) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev)

          const isDefault = (key === 'dateRange' && value === '24h') || (key === 'period' && value === 'hourly')

          if (isDefault) {
            next.delete(key)
          } else {
            next.set(key, value)
          }

          return next
        },
        { replace: true, state }
      )
    },
    [setSearchParams, state]
  )

  return { filters, setFilter }
}
