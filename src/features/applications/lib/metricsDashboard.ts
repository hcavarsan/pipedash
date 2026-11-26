import type { MetricResult, MetricUnit } from '../../../types/applications'
import type { AggregationPeriod, MetricsDashboard, MetricType } from '../../../types/metrics'
import { formatMetricValue } from '../../metrics/lib/metricsUtils'

export const UNIT_METRIC_TYPE: Record<MetricUnit, MetricType> = {
  seconds: 'run_duration',
  percent: 'success_rate',
  count: 'run_frequency',
  commits: 'run_frequency',
}

export function formatApplicationMetric(value: number | null, unit: MetricUnit): string {
  return formatMetricValue(value, UNIT_METRIC_TYPE[unit])
}

export function toMetricsDashboard(result: MetricResult, period: AggregationPeriod): MetricsDashboard {
  const metrics = result.series.map((point) => ({ ...point, min: null, max: null, avg: point.value }))

  return {
    data: {
      metrics,
      total_count: metrics.length,
      metric_type: UNIT_METRIC_TYPE[result.unit],
      aggregation_period: period,
    },
    observed: result.series.map((point) => point.count > 0),
    stats: { average: null, latest: null, max: null, observed: metrics.length, total: metrics.length },
    table_indices: [],
    ranges: [],
    widest_range: '90d',
    aggregation_type: 'avg',
  }
}
