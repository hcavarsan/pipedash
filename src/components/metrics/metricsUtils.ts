import { useMemo } from 'react'

import type { AggregatedMetric, AggregatedMetrics, AggregationPeriod, AggregationType, MetricType } from '../../types'
import { DATE_RANGE_OPTIONS, type DateRange, getDateRangeLabel, getDateRangeStart } from '../../utils/dateRange'
import { formatDuration } from '../../utils/formatDuration'

const MS_PER_DAY = 24 * 60 * 60 * 1000

export const METRIC_CONFIG: Record<
  MetricType,
  { label: string; valueLabel: string; subject: string; color: 'blue' | 'green' }
> = {
  run_duration: { label: 'Run duration', valueLabel: 'Duration', subject: 'run duration', color: 'blue' },
  success_rate: { label: 'Success rate', valueLabel: 'Success rate', subject: 'success rate', color: 'green' },
  run_frequency: { label: 'Run frequency', valueLabel: 'Runs', subject: 'runs', color: 'blue' },
}

export const METRIC_TYPES: MetricType[] = ['run_duration', 'success_rate', 'run_frequency']

export const AGGREGATION_PERIOD_OPTIONS: { value: AggregationPeriod; label: string }[] = [
  { value: 'hourly', label: 'Hourly' },
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'monthly', label: 'Monthly' },
]

export const AGGREGATION_TYPE_OPTIONS: { value: AggregationType; label: string }[] = [
  { value: 'avg', label: 'Average' },
  { value: 'sum', label: 'Total' },
  { value: 'min', label: 'Minimum' },
  { value: 'max', label: 'Maximum' },
  { value: 'p95', label: '95th percentile' },
  { value: 'p99', label: '99th percentile' },
]

const PERIOD_UNIT: Record<AggregationPeriod, string> = {
  hourly: 'hour',
  daily: 'day',
  weekly: 'week',
  monthly: 'month',
}

export const isAggregationPeriod = (value: string | null | undefined): value is AggregationPeriod =>
  AGGREGATION_PERIOD_OPTIONS.some((option) => option.value === value)

export const isAggregationType = (value: string | null | undefined): value is AggregationType =>
  AGGREGATION_TYPE_OPTIONS.some((option) => option.value === value)

export const getDefaultAggregationType = (metricType: MetricType): AggregationType =>
  metricType === 'run_frequency' ? 'sum' : 'avg'

export const buildChartTitle = (
  metricType: MetricType,
  aggregationType: AggregationType,
  period: AggregationPeriod,
  range: DateRange
): string => {
  const aggregation = AGGREGATION_TYPE_OPTIONS.find((option) => option.value === aggregationType)?.label

  return `${aggregation} ${METRIC_CONFIG[metricType].subject} per ${PERIOD_UNIT[period]} · ${getDateRangeLabel(range)}`
}

export const getRangeOptions = (retentionDays: number) => {
  const now = new Date()

  return DATE_RANGE_OPTIONS.map((option) => {
    const start = getDateRangeStart(option.value, now)
    const disabled = start !== null && (now.getTime() - start.getTime()) / MS_PER_DAY > retentionDays

    return {
      value: option.value,
      label: disabled ? `${option.label} (beyond retention)` : option.label,
      disabled,
    }
  })
}

export const getWidestRange = (retentionDays: number): DateRange =>
  getRangeOptions(retentionDays).findLast((option) => !option.disabled)?.value ?? '24h'

export const useMetricsWindow = (range: DateRange) =>
  useMemo(() => {
    const now = new Date()

    return { startDate: getDateRangeStart(range, now)?.toISOString(), endDate: now.toISOString() }
  }, [range])

export const hasObservation = (metric: AggregatedMetric, metricType: MetricType): boolean =>
  metricType === 'run_frequency' || metric.count > 0

const COUNT_FORMAT = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 })
const PERCENT_FORMAT = new Intl.NumberFormat(undefined, {
  style: 'unit',
  unit: 'percent',
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
})

export const formatNumber = (value: number): string => COUNT_FORMAT.format(value)

export const formatMetricValue = (value: number | null, metricType: MetricType): string => {
  if (value === null) {
    return '—'
  }

  switch (metricType) {
    case 'run_duration':
      return value > 0 ? formatDuration(value) : '—'
    case 'success_rate':
      return PERCENT_FORMAT.format(value)
    case 'run_frequency':
      return COUNT_FORMAT.format(value)
  }
}

interface MetricStats {
  average: number | null
  latest: number | null
  max: number | null
  observed: number
  total: number
}

export const getMetricStats = ({ metrics, metric_type }: AggregatedMetrics): MetricStats => {
  const values = metrics.filter((metric) => hasObservation(metric, metric_type)).map((metric) => metric.value)

  if (values.length === 0) {
    return { average: null, latest: null, max: null, observed: 0, total: metrics.length }
  }

  return {
    average: values.reduce((sum, value) => sum + value, 0) / values.length,
    latest: values[values.length - 1],
    max: Math.max(...values),
    observed: values.length,
    total: metrics.length,
  }
}
