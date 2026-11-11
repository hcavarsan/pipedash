import type { DateRangeValue, SortDirection } from './views'

export type MetricType = 'run_duration' | 'success_rate' | 'run_frequency'

export type AggregationPeriod = 'hourly' | 'daily' | 'weekly' | 'monthly'

export type AggregationType = 'avg' | 'sum' | 'min' | 'max' | 'p95' | 'p99'

export interface GlobalMetricsConfig {
  enabled: boolean
  default_retention_days: number
  updated_at: string
}

export interface MetricsConfig {
  pipeline_id: string
  enabled: boolean
  retention_days: number
  created_at: string
  updated_at: string
}

export interface AggregatedMetric {
  timestamp: string
  value: number
  count: number
  min: number | null
  max: number | null
  avg: number
}

export interface AggregatedMetrics {
  metrics: AggregatedMetric[]
  total_count: number
  metric_type: MetricType
  aggregation_period: AggregationPeriod
}

export interface PipelineMetricsStats {
  pipeline_id: string
  pipeline_name: string
  metrics_count: number
  oldest_metric: string | null
  newest_metric: string | null
}

export interface MetricsStats {
  total_metrics_count: number
  estimated_size_bytes: number
  estimated_size_mb: number
  last_cleanup_at: string | null
  updated_at: string
  by_pipeline: PipelineMetricsStats[]
}

export type MetricSortField = 'timestamp' | 'value' | 'count' | 'min' | 'max'

export interface MetricTableSort {
  field: MetricSortField
  direction: SortDirection
}

export interface MetricsDashboardQuery {
  pipeline_id?: string | undefined
  metric_type: MetricType
  aggregation_period: AggregationPeriod
  aggregation_type?: AggregationType | undefined
  date_range: DateRangeValue
  utc_offset_minutes: number
  limit?: number | undefined
  table_sort?: MetricTableSort | undefined
}

export interface MetricStats {
  average: number | null
  latest: number | null
  max: number | null
  observed: number
  total: number
}

export interface MetricRangeOption {
  value: DateRangeValue
  enabled: boolean
}

export interface MetricsDashboard {
  data: AggregatedMetrics
  observed: boolean[]
  stats: MetricStats
  table_indices: number[]
  ranges: MetricRangeOption[]
  widest_range: DateRangeValue
  aggregation_type: AggregationType
}
