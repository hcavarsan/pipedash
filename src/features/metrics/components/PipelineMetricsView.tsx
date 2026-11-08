import { Button, Group, SegmentedControl, Select, Stack } from '@mantine/core'
import { IconAdjustments, IconChartLine } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { StandardModal } from '../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useMetricsFilters } from '../../../hooks/useUrlState'
import { runHistoryQuery } from '../../../lib/runHistoryQuery'
import { useRunHistory } from '../../../queries/useRunHistoryQueries'
import { service } from '../../../services/service'
import type { AggregationPeriod, MetricType } from '../../../types/metrics'
import { type DateRange, isDateRange } from '../../../utils/dateRange'
import { formatCount } from '../../../utils/formatCount'
import { useMetricsEventListeners } from '../hooks/useMetricsEventListeners'
import {
  AGGREGATION_PERIOD_OPTIONS,
  getDefaultAggregationType,
  isAggregationPeriod,
  METRIC_CONFIG,
  toRangeOptions,
} from '../lib/metricsUtils'
import {
  useInvalidatePipelineMetrics,
  useMetricsDashboard,
  usePipelineMetricsConfig,
} from '../queries/useMetricsQueries'
import { MetricsConfigModal } from './MetricsConfigModal'
import { MetricsDetailPage } from './MetricsDetailPage'
import { MetricsPickerRow } from './MetricsPickerRow'
import { PipelineMetricCards } from './PipelineMetricCards'

interface PipelineMetricsViewProps {
  pipelineId: string
  pipelineName: string
  repository?: string | undefined
}

export const PipelineMetricsView = ({ pipelineId, pipelineName, repository }: PipelineMetricsViewProps): ReactNode => {
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

  const queryParams = { aggregationPeriod, pipelineId, dateRange, enabled: isMetricsEnabled }

  const durationQuery = useMetricsDashboard({
    ...queryParams,
    metricType: 'run_duration',
    aggregationType: getDefaultAggregationType('run_duration'),
  })
  const successRateQuery = useMetricsDashboard({
    ...queryParams,
    metricType: 'success_rate',
    aggregationType: getDefaultAggregationType('success_rate'),
  })
  const frequencyQuery = useMetricsDashboard({
    ...queryParams,
    metricType: 'run_frequency',
    aggregationType: getDefaultAggregationType('run_frequency'),
  })

  const metricQueries: Record<MetricType, typeof durationQuery> = {
    run_duration: durationQuery,
    success_rate: successRateQuery,
    run_frequency: frequencyQuery,
  }

  const rangeSource = durationQuery.data?.ranges
  const refetchAll = useInvalidatePipelineMetrics(pipelineId)

  useRunHistory(pipelineId, runHistoryQuery(1, 50), isMetricsEnabled)

  const handleConfigSaved = async () => {
    const { data: freshConfig } = await refetchConfig()

    if (freshConfig?.enabled) {
      await service.fetchRunHistory(pipelineId, runHistoryQuery(1, 50))

      void refetchAll()
    }
  }

  const handleConfigChanged = () => {
    void refetchConfig().then(refetchAll)
  }

  useMetricsEventListeners({
    pipelineId,
    enabled: isMetricsEnabled,
    onConfigChanged: handleConfigChanged,
  })

  const handleExpandMetric = (metricType: MetricType) => {
    setSelectedMetricType(metricType)
    setShowDetailPage(true)
  }

  const cards = (
    <PipelineMetricCards
      queries={metricQueries}
      dateRange={dateRange}
      aggregationPeriod={aggregationPeriod}
      onRangeChange={(range) => setFilter('dateRange', range)}
      onRefresh={refetchAll}
      onExpand={handleExpandMetric}
    />
  )

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
          icon={<IconChartLine size={24} aria-hidden />}
          title="Metrics are turned off for this pipeline"
          description="Turn on metrics collection to track run duration, success rate, and run frequency over time."
          action={
            <Button
              size="sm"
              leftSection={<IconAdjustments size={16} aria-hidden />}
              onClick={() => setConfigModalOpened(true)}
            >
              Enable metrics
            </Button>
          }
        />
      )
    }

    if (mobileShell) {
      const filters = (
        <Stack gap="sm">
          <IosGroup>
            <MetricsPickerRow
              label="Time range"
              value={dateRange}
              onChange={(value) => isDateRange(value) && setFilter('dateRange', value)}
              data={toRangeOptions(rangeSource)}
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
      )

      return (
        <Stack gap="lg" pb="md">
          {filters}

          {cards}

          <IosGroup footer={`Metrics are kept for ${formatCount(retentionDays, 'day')}.`}>
            <IosRow
              title="Configure metrics"
              icon={<IconAdjustments size={18} aria-hidden />}
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
              data={toRangeOptions(rangeSource)}
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
            leftSection={<IconAdjustments size={16} aria-hidden />}
            onClick={() => setConfigModalOpened(true)}
          >
            Configure metrics
          </Button>
        </Group>

        {cards}
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
        <ErrorBoundary scope="feature" name="Metric details" resetKeys={[selectedMetricType]}>
          <MetricsDetailPage
            pipelineId={pipelineId}
            pipelineName={pipelineName}
            repository={repository}
            metricType={selectedMetricType}
            initialDateRange={dateRange}
            initialAggregation={aggregationPeriod}
          />
        </ErrorBoundary>
      </StandardModal>

      <MetricsConfigModal
        scope="pipeline"
        opened={configModalOpened}
        onClose={() => setConfigModalOpened(false)}
        onConfigChange={handleConfigSaved}
        pipelineId={pipelineId}
        pipelineName={pipelineName}
      />
    </>
  )
}
