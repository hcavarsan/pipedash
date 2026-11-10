import { Grid, Stack } from '@mantine/core'
import type { UseQueryResult } from '@tanstack/react-query'
import { Fragment, type ReactNode, useRef } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { AggregationPeriod, MetricsDashboard, MetricType } from '../../../types/metrics'
import type { DateRange } from '../../../utils/dateRange'
import { buildChartTitle, getDefaultAggregationType, METRIC_CONFIG, METRIC_TYPES } from '../lib/metricsUtils'
import { CompactMetricsCard } from './CompactMetricsCard'
import { MetricsEmptyState } from './MetricsEmptyState'

interface PipelineMetricCardsProps {
  queries: Record<MetricType, UseQueryResult<MetricsDashboard, Error>>
  dateRange: DateRange
  aggregationPeriod: AggregationPeriod
  onRangeChange: (range: DateRange) => void
  onRefresh: () => void | Promise<void>
  onExpand: (metricType: MetricType) => void
}

interface MetricCardProps {
  type: MetricType
  query: UseQueryResult<MetricsDashboard, Error>
  dateRange: DateRange
  aggregationPeriod: AggregationPeriod
  onExpand: (metricType: MetricType) => void
}

function MetricCard({ type, query, dateRange, aggregationPeriod, onExpand }: MetricCardProps): ReactNode {
  const resolved = useRef({ aggregationPeriod, dateRange })
  const { data, isPlaceholderData } = query

  if (data && !isPlaceholderData) {
    resolved.current = { aggregationPeriod, dateRange }
  }

  const error = query.isError && (
    <ErrorState
      title={`Couldn't load ${METRIC_CONFIG[type].label.toLowerCase()}`}
      error={query.error}
      onRetry={() => void query.refetch()}
      retrying={query.isFetching}
    />
  )

  if (error && !data) {
    return error
  }

  const hasMetrics = !!data && data.data.metrics.length > 0

  return (
    <Stack gap="xs" h="100%" aria-busy={isPlaceholderData}>
      {error}
      {isPlaceholderData && <LoadingState variant="compact" message="Updating metrics…" />}
      <ErrorBoundary scope="feature" name={METRIC_CONFIG[type].label}>
        <CompactMetricsCard
          metricType={type}
          data={data}
          title={buildChartTitle(
            type,
            getDefaultAggregationType(type),
            resolved.current.aggregationPeriod,
            resolved.current.dateRange
          )}
          loading={query.isPending || (isPlaceholderData && !hasMetrics)}
          onExpand={() => onExpand(type)}
        />
      </ErrorBoundary>
    </Stack>
  )
}

export function PipelineMetricCards({
  queries,
  dateRange,
  aggregationPeriod,
  onRangeChange,
  onRefresh,
  onExpand,
}: PipelineMetricCardsProps): ReactNode {
  const mobileShell = useMobileShell()
  const allQueries = METRIC_TYPES.map((type) => queries[type])
  const isRefetching = allQueries.some((query) => query.isFetching)
  const resolvedEmpty =
    allQueries.every((query) => query.isSuccess && !query.isPlaceholderData) &&
    !allQueries.some((query) => query.data && query.data.data.metrics.length > 0)

  if (resolvedEmpty) {
    return (
      <MetricsEmptyState
        range={dateRange}
        widest={allQueries.find((query) => query.data)?.data?.widest_range ?? dateRange}
        onRangeChange={onRangeChange}
        onRefresh={onRefresh}
        refreshing={isRefetching}
      />
    )
  }

  const renderCard = (type: MetricType) => (
    <MetricCard
      type={type}
      query={queries[type]}
      dateRange={dateRange}
      aggregationPeriod={aggregationPeriod}
      onExpand={onExpand}
    />
  )

  if (mobileShell) {
    return (
      <Stack gap="md">
        {METRIC_TYPES.map((type) => (
          <Fragment key={type}>{renderCard(type)}</Fragment>
        ))}
      </Stack>
    )
  }

  return (
    <Grid gap="md">
      {METRIC_TYPES.map((type) => (
        <Grid.Col key={type} span={{ base: 12, md: 4 }}>
          {renderCard(type)}
        </Grid.Col>
      ))}
    </Grid>
  )
}
