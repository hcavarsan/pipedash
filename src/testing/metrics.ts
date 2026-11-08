import type { AggregatedMetrics, MetricsDashboard } from '../types/metrics'

export function dashboardFixture(data: AggregatedMetrics, overrides: Partial<MetricsDashboard> = {}): MetricsDashboard {
  return {
    data,
    observed: data.metrics.map(() => true),
    stats: { average: null, latest: null, max: null, observed: data.metrics.length, total: data.metrics.length },
    table_indices: data.metrics.map((_, index) => index),
    ranges: [],
    widest_range: '24h',
    aggregation_type: 'avg',
    ...overrides,
  }
}
