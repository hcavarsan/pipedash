import { Button, Grid, Group, SegmentedControl, Select, Stack } from '@mantine/core'
import { IconAdjustments, IconChartLine } from '@tabler/icons-react'
import { Fragment, useCallback, useEffect, useRef, useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useMetricsEventListeners } from '../../hooks/useMetricsEventListeners'
import { useMetricsFilters } from '../../hooks/useUrlState'
import { logger } from '../../lib/logger'
import {
  useAggregatedMetrics,
  useInvalidatePipelineMetrics,
  usePipelineMetricsConfig,
} from '../../queries/useMetricsQueries'
import { service } from '../../services'
import type { AggregationPeriod, MetricType } from '../../types'
import { type DateRange, isDateRange } from '../../utils/dateRange'
import { formatCount } from '../../utils/formatCount'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'
import { CompactMetricsCard } from '../metrics/CompactMetricsCard'
import { MetricsConfigModal } from '../metrics/MetricsConfigModal'
import { MetricsDetailPage } from '../metrics/MetricsDetailPage'
import { MetricsEmptyState } from '../metrics/MetricsEmptyState'
import { MetricsPickerRow } from '../metrics/MetricsPickerRow'
import {
  AGGREGATION_PERIOD_OPTIONS,
  buildChartTitle,
  getDefaultAggregationType,
  getRangeOptions,
  isAggregationPeriod,
  METRIC_CONFIG,
  METRIC_TYPES,
  useMetricsWindow,
} from '../metrics/metricsUtils'

interface PipelineMetricsViewProps {
  pipelineId: string
  pipelineName: string
  repository?: string
  refreshTrigger?: number
}

export const PipelineMetricsView = ({
  pipelineId,
  pipelineName,
  repository,
  refreshTrigger,
}: PipelineMetricsViewProps) => {
  const mobileShell = useMobileShell()
  const { filters, setFilter } = useMetricsFilters()
  const dateRange: DateRange = isDateRange(filters.dateRange) ? filters.dateRange : '24h'
  const aggregationPeriod: AggregationPeriod = isAggregationPeriod(filters.period) ? filters.period : 'hourly'

  const [configModalOpened, setConfigModalOpened] = useState(false)
  const [showDetailPage, setShowDetailPage] = useState(false)
  const [selectedMetricType, setSelectedMetricType] = useState<MetricType>('run_duration')

  const pipelineConfig = usePipelineMetricsConfig(pipelineId)
  const { refetch: refetchConfig } = pipelineConfig
  const isMetricsEnabled = pipelineConfig.data?.enabled ?? false
  const retentionDays = pipelineConfig.data?.retention_days ?? 7

  const { startDate, endDate } = useMetricsWindow(dateRange)
  const queryParams = { aggregationPeriod, pipelineId, startDate, endDate, enabled: isMetricsEnabled }

  const durationQuery = useAggregatedMetrics({
    ...queryParams,
    metricType: 'run_duration',
    aggregationType: getDefaultAggregationType('run_duration'),
  })
  const successRateQuery = useAggregatedMetrics({
    ...queryParams,
    metricType: 'success_rate',
    aggregationType: getDefaultAggregationType('success_rate'),
  })
  const frequencyQuery = useAggregatedMetrics({
    ...queryParams,
    metricType: 'run_frequency',
    aggregationType: getDefaultAggregationType('run_frequency'),
  })

  const metricQueries: Record<MetricType, typeof durationQuery> = {
    run_duration: durationQuery,
    success_rate: successRateQuery,
    run_frequency: frequencyQuery,
  }
  const allQueries = METRIC_TYPES.map((type) => metricQueries[type])

  const refetchAll = useInvalidatePipelineMetrics(pipelineId)

  useEffect(() => {
    if (refreshTrigger !== undefined && refreshTrigger > 0 && isMetricsEnabled) {
      refetchAll()
    }
  }, [refreshTrigger, isMetricsEnabled, refetchAll])

  const hasTriggeredCollectionRef = useRef<string | null>(null)

  useEffect(() => {
    if (isMetricsEnabled && hasTriggeredCollectionRef.current !== pipelineId) {
      hasTriggeredCollectionRef.current = pipelineId

      service.fetchRunHistory(pipelineId, 1, 50).catch((error) => {
        logger.warn('PipelineMetricsView', 'Failed to trigger metrics collection', error)
      })
    }
  }, [pipelineId, isMetricsEnabled])

  const handleConfigSaved = async () => {
    const { data: freshConfig } = await refetchConfig()

    if (freshConfig?.enabled) {
      await service.clearRunHistoryCache(pipelineId)
      await service.fetchRunHistory(pipelineId, 1, 50)

      refetchAll()
    }
  }

  const handleConfigChanged = useCallback(() => {
    refetchConfig().then(refetchAll)
  }, [refetchConfig, refetchAll])

  useMetricsEventListeners({
    pipelineId,
    enabled: isMetricsEnabled,
    onConfigChanged: handleConfigChanged,
  })

  const handleExpandMetric = (metricType: MetricType) => {
    setSelectedMetricType(metricType)
    setShowDetailPage(true)
  }

  const isRefetching = allQueries.some((query) => query.isFetching)
  const failedQuery = allQueries.find((query) => query.isError)

  const renderMetrics = () => {
    if (failedQuery) {
      return (
        <ErrorState
          title="Couldn't load pipeline metrics"
          error={failedQuery.error}
          onRetry={refetchAll}
          retrying={isRefetching}
        />
      )
    }

    if (allQueries.some((query) => query.isPending)) {
      return <LoadingState message="Loading pipeline metrics..." />
    }

    if (!allQueries.some((query) => query.data && query.data.metrics.length > 0)) {
      return (
        <MetricsEmptyState
          range={dateRange}
          retentionDays={retentionDays}
          onRangeChange={(range) => setFilter('dateRange', range)}
          onRefresh={refetchAll}
          refreshing={isRefetching}
        />
      )
    }

    const renderCard = (type: MetricType) => (
      <CompactMetricsCard
        metricType={type}
        data={metricQueries[type].data}
        title={buildChartTitle(type, getDefaultAggregationType(type), aggregationPeriod, dateRange)}
        loading={metricQueries[type].isPending}
        onExpand={() => handleExpandMetric(type)}
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

  const renderContent = () => {
    if (pipelineConfig.isError) {
      return (
        <ErrorState
          title="Couldn't load metrics settings"
          error={pipelineConfig.error}
          onRetry={() => refetchConfig()}
          retrying={pipelineConfig.isFetching}
        />
      )
    }

    if (pipelineConfig.isPending) {
      return <LoadingState message="Loading metrics settings..." />
    }

    if (!isMetricsEnabled) {
      return (
        <EmptyState
          icon={<IconChartLine size={24} />}
          title="Metrics are turned off for this pipeline"
          description="Turn on metrics collection to track run duration, success rate, and run frequency over time."
          action={
            <Button size="sm" leftSection={<IconAdjustments size={16} />} onClick={() => setConfigModalOpened(true)}>
              Enable metrics
            </Button>
          }
        />
      )
    }

    if (mobileShell) {
      return (
        <Stack gap="lg" pb="md">
          <Stack gap="sm">
            <IosGroup>
              <MetricsPickerRow
                label="Time range"
                value={dateRange}
                onChange={(value) => isDateRange(value) && setFilter('dateRange', value)}
                data={getRangeOptions(retentionDays)}
              />
            </IosGroup>
            <SegmentedControl
              fullWidth
              aria-label="Group by"
              value={aggregationPeriod}
              onChange={(value) => isAggregationPeriod(value) && setFilter('period', value)}
              data={AGGREGATION_PERIOD_OPTIONS}
            />
          </Stack>

          {renderMetrics()}

          <IosGroup footer={`Metrics are kept for ${formatCount(retentionDays, 'day')}.`}>
            <IosRow
              title="Configure metrics"
              icon={<IconAdjustments size={18} />}
              chevron
              onClick={() => setConfigModalOpened(true)}
            />
          </IosGroup>
        </Stack>
      )
    }

    return (
      <Stack gap="md">
        <Group justify="space-between" align="flex-end" wrap="wrap">
          <Group gap="sm" wrap="wrap">
            <Select
              label="Time range"
              value={dateRange}
              onChange={(value) => isDateRange(value) && setFilter('dateRange', value)}
              data={getRangeOptions(retentionDays)}
              allowDeselect={false}
              w={220}
            />
            <Select
              label="Group by"
              value={aggregationPeriod}
              onChange={(value) => isAggregationPeriod(value) && setFilter('period', value)}
              data={AGGREGATION_PERIOD_OPTIONS}
              allowDeselect={false}
              w={130}
            />
          </Group>
          <Button
            size="sm"
            variant="default"
            leftSection={<IconAdjustments size={16} />}
            onClick={() => setConfigModalOpened(true)}
          >
            Configure metrics
          </Button>
        </Group>

        {renderMetrics()}
      </Stack>
    )
  }

  return (
    <>
      {renderContent()}

      <StandardModal
        opened={showDetailPage}
        onClose={() => setShowDetailPage(false)}
        size="xl"
        title={
          mobileShell
            ? METRIC_CONFIG[selectedMetricType].label
            : `${METRIC_CONFIG[selectedMetricType].label} · ${repository ? `${repository} / ` : ''}${pipelineName}`
        }
      >
        <MetricsDetailPage
          pipelineId={pipelineId}
          pipelineName={pipelineName}
          repository={repository}
          metricType={selectedMetricType}
          initialDateRange={dateRange}
          initialAggregation={aggregationPeriod}
          retentionDays={retentionDays}
          refreshTrigger={refreshTrigger}
        />
      </StandardModal>

      <MetricsConfigModal
        opened={configModalOpened}
        onClose={() => setConfigModalOpened(false)}
        onConfigChange={handleConfigSaved}
        pipelineId={pipelineId}
        pipelineName={pipelineName}
      />
    </>
  )
}
