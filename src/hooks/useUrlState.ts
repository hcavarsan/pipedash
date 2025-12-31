import { useCallback, useMemo } from 'react'
import { useSearchParams } from 'react-router-dom'

export interface PipelineFilters {
  search: string
  status: string | null
  provider: string | null
  organization: string | null
  dateRange: string | null
}

export function usePipelineFilters(): {
  filters: PipelineFilters
  setFilter: <K extends keyof PipelineFilters>(key: K, value: PipelineFilters[K]) => void
  clearFilters: () => void
} {
  const [searchParams, setSearchParams] = useSearchParams()

  const filters = useMemo((): PipelineFilters => ({
    search: searchParams.get('search') ?? '',
    status: searchParams.get('status'),
    provider: searchParams.get('provider'),
    organization: searchParams.get('organization'),
    dateRange: searchParams.get('dateRange'),
  }), [searchParams])

  const setFilter = useCallback(
    <K extends keyof PipelineFilters>(key: K, value: PipelineFilters[K]) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev)

          if (value === null || value === '') {
            next.delete(key)
          } else {
            next.set(key, String(value))
          }

          return next
        },
        { replace: true }
      )
    },
    [setSearchParams]
  )

  const clearFilters = useCallback(() => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev)


        next.delete('search')
        next.delete('status')
        next.delete('provider')
        next.delete('organization')
        next.delete('dateRange')

        return next
      },
      { replace: true }
    )
  }, [setSearchParams])

  return { filters, setFilter, clearFilters }
}

export interface RunHistoryFilters {
  search: string
  status: string | null
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

  const filters = useMemo((): RunHistoryFilters => ({
    search: searchParams.get('search') ?? '',
    status: searchParams.get('status'),
    branch: searchParams.get('branch'),
    actor: searchParams.get('actor'),
    dateRange: searchParams.get('dateRange'),
    page: Number(searchParams.get('page')) || 1,
  }), [searchParams])

  const setFilter = useCallback(
    <K extends keyof RunHistoryFilters>(key: K, value: RunHistoryFilters[K]) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev)

          const isDefault =
            (key === 'search' && value === '') ||
            (key === 'page' && value === 1) ||
            value === null

          if (isDefault) {
            next.delete(key)
          } else {
            next.set(key, String(value))
          }

          return next
        },
        { replace: true }
      )
    },
    [setSearchParams]
  )

  const clearFilters = useCallback(() => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev)


        next.delete('search')
        next.delete('status')
        next.delete('branch')
        next.delete('actor')
        next.delete('dateRange')
        next.delete('page')

        return next
      },
      { replace: true }
    )
  }, [setSearchParams])

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

  const filters = useMemo((): MetricsFilters => ({
    dateRange: searchParams.get('dateRange') ?? '24h',
    period: searchParams.get('period') ?? 'hourly',
  }), [searchParams])

  const setFilter = useCallback(
    <K extends keyof MetricsFilters>(key: K, value: MetricsFilters[K]) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev)

          const isDefault =
            (key === 'dateRange' && value === '24h') ||
            (key === 'period' && value === 'hourly')

          if (isDefault) {
            next.delete(key)
          } else {
            next.set(key, value)
          }

          return next
        },
        { replace: true }
      )
    },
    [setSearchParams]
  )

  return { filters, setFilter }
}
