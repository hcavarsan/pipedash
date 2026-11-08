import { Grid, Stack } from '@mantine/core'
import type { UseQueryResult } from '@tanstack/react-query'
import { Fragment, type ReactNode } from 'react'
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
  const failedQuery = allQueries.find((query) => query.isError)

  if (failedQuery) {
    return (
      <ErrorState
        title="Couldn't load pipeline metrics"
        error={failedQuery.error}
        onRetry={onRefresh}
        retrying={isRefetching}
      />
    )
  }

  if (allQueries.some((query) => query.isPending)) {
    return <LoadingState message="Loading pipeline metrics..." />
  }

  if (!allQueries.some((query) => query.data && query.data.data.metrics.length > 0)) {
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
    <ErrorBoundary scope="feature" name={METRIC_CONFIG[type].label}>
      <CompactMetricsCard
        metricType={type}
        data={queries[type].data}
        title={buildChartTitle(type, getDefaultAggregationType(type), aggregationPeriod, dateRange)}
        loading={queries[type].isPending}
        onExpand={() => onExpand(type)}
      />
    </ErrorBoundary>
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
