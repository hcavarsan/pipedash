export const DATE_RANGE_OPTIONS = [
  { value: 'today', label: 'Today' },
  { value: '24h', label: 'Last 24 hours' },
  { value: '7d', label: 'Last 7 days' },
  { value: '30d', label: 'Last 30 days' },
  { value: '60d', label: 'Last 60 days' },
  { value: '90d', label: 'Last 90 days' },
] as const

export type DateRange = (typeof DATE_RANGE_OPTIONS)[number]['value']

const RANGE_DAYS: Partial<Record<DateRange, number>> = { '7d': 7, '30d': 30, '60d': 60, '90d': 90 }

export const isDateRange = (value: string | null | undefined): value is DateRange =>
  DATE_RANGE_OPTIONS.some((option) => option.value === value)

export const getDateRangeStart = (range: string | null | undefined, now = new Date()): Date | null => {
  if (!isDateRange(range)) {
    return null
  }

  const start = new Date(now)

  if (range === 'today') {
    start.setHours(0, 0, 0, 0)
  } else if (range === '24h') {
    start.setTime(now.getTime() - 24 * 60 * 60 * 1000)
  } else {
    start.setDate(start.getDate() - (RANGE_DAYS[range] ?? 0))
  }

  return start
}

export const getDateRangeLabel = (range: DateRange): string =>
  DATE_RANGE_OPTIONS.find((option) => option.value === range)?.label ?? range
