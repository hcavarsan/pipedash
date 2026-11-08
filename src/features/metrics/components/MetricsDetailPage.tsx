import { Button, Group, Loader, Menu, SegmentedControl, Stack, UnstyledButton } from '@mantine/core'
import { IconDownload, IconFileTypeCsv, IconPhoto } from '@tabler/icons-react'
import { type ReactNode, useRef, useState } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { FilterSelect } from '../../../components/common/FilterSelect'
import { LoadingState } from '../../../components/common/LoadingState'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import { IosGroup } from '../../../components/ios/IosList'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import type { AggregationPeriod, AggregationType, MetricTableSort, MetricType } from '../../../types/metrics'
import { type DateRange, isDateRange } from '../../../utils/dateRange'
import { useExportMetrics } from '../hooks/useExportMetrics'
import {
  AGGREGATION_PERIOD_OPTIONS,
  AGGREGATION_TYPE_OPTIONS,
  buildChartTitle,
  getDefaultAggregationType,
  isAggregationPeriod,
  isAggregationType,
  METRIC_CONFIG,
  toRangeOptions,
} from '../lib/metricsUtils'
import { useMetricsDashboard } from '../queries/useMetricsQueries'
import { LazyMetricsChart } from './LazyMetricsChart'
import { MetricsDataTable } from './MetricsDataTable'
import { MetricsEmptyState } from './MetricsEmptyState'
import { MetricsPickerRow } from './MetricsPickerRow'
import classes from './metrics.module.css'

interface MetricsDetailPageProps {
  pipelineId: string
  pipelineName: string
  metricType: MetricType
  repository?: string | undefined
  initialDateRange: DateRange
  initialAggregation: AggregationPeriod
}

export const MetricsDetailPage = ({
  pipelineId,
  pipelineName,
  repository,
  metricType,
  initialDateRange,
  initialAggregation,
}: MetricsDetailPageProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const [dateRange, setDateRange] = useState<DateRange>(initialDateRange)
  const [aggregationPeriod, setAggregationPeriod] = useState<AggregationPeriod>(initialAggregation)
  const [aggregationType, setAggregationType] = useState<AggregationType>(getDefaultAggregationType(metricType))
  const [selectedDataPointIndex, setSelectedDataPointIndex] = useState<number | null>(null)
  const chartRef = useRef<HTMLDivElement>(null)

  const [prevMetricType, setPrevMetricType] = useState(metricType)

  if (prevMetricType !== metricType) {
    setPrevMetricType(metricType)
    setAggregationType(getDefaultAggregationType(metricType))
  }

  const [tableSort, setTableSort] = useState<MetricTableSort | undefined>(undefined)
  const metricsQuery = useMetricsDashboard({
    metricType,
    aggregationPeriod,
    aggregationType,
    pipelineId,
    dateRange,
    tableSort,
  })
  const { data, refetch } = metricsQuery

  const [prevData, setPrevData] = useState(data)

  if (prevData !== data) {
    setPrevData(data)
    setSelectedDataPointIndex(null)
  }

  const fileBaseName = `${pipelineName.replace(/[^a-z0-9]/gi, '_')}_${metricType}_${dateRange}`
  const exportMetrics = useExportMetrics({ data, metricType, fileBaseName, chartRef })
  const exporting = exportMetrics.isPending
  const hasData = !!data && data.data.metrics.length > 0
  const canExport = hasData && !metricsQuery.isFetching && !exporting
  const metricLabel = METRIC_CONFIG[metricType].label.toLowerCase()

  let bodyGap = 'md'
  if (mobileShell) {
    bodyGap = 'lg'
  } else if (isMobile) {
    bodyGap = 'sm'
  }

  const renderBody = () => {
    if (metricsQuery.isError) {
      return (
        <ErrorState
          title={`Couldn't load ${metricLabel} metrics`}
          error={metricsQuery.error}
          onRetry={() => refetch()}
          retrying={metricsQuery.isFetching}
        />
      )
    }

    if (metricsQuery.isPending) {
      return <LoadingState message={`Loading ${metricLabel} metrics...`} />
    }

    if (!data || data.data.metrics.length === 0) {
      return (
        <MetricsEmptyState
          range={dateRange}
          widest={data?.widest_range ?? dateRange}
          onRangeChange={setDateRange}
          onRefresh={() => refetch()}
          refreshing={metricsQuery.isFetching}
        />
      )
    }

    return (
      <Stack gap={bodyGap}>
        <LazyMetricsChart
          data={data}
          selectedIndex={selectedDataPointIndex}
          onDataPointClick={setSelectedDataPointIndex}
          chartTitle={buildChartTitle(metricType, aggregationType, aggregationPeriod, dateRange)}
          chartRef={chartRef}
        />
        <MetricsDataTable
          data={data}
          sort={tableSort}
          onSortChange={setTableSort}
          selectedIndex={selectedDataPointIndex}
          onRowClick={setSelectedDataPointIndex}
        />
      </Stack>
    )
  }

  const exportMenuItems = (
    <Menu.Dropdown>
      <Menu.Item leftSection={<IconPhoto size={16} aria-hidden />} onClick={() => exportMetrics.mutate('png')}>
        Export as PNG
      </Menu.Item>
      <Menu.Item leftSection={<IconFileTypeCsv size={16} aria-hidden />} onClick={() => exportMetrics.mutate('csv')}>
        Export as CSV
      </Menu.Item>
      <ActionSheetCancel />
    </Menu.Dropdown>
  )

  const mobileExportButton = (
    <UnstyledButton
      className={classes.exportButton}
      disabled={!canExport}
      aria-label="Export metrics"
      aria-busy={exporting}
    >
      {exporting ? <Loader size={18} /> : <IconDownload size={18} aria-hidden />}
      {exporting ? 'Exporting…' : 'Export'}
    </UnstyledButton>
  )

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <div className={classes.detailHeader}>
          <div className={classes.caption}>{repository ? `${repository} / ${pipelineName}` : pipelineName}</div>
          <Menu position="bottom" disabled={!canExport}>
            <Menu.Target>{mobileExportButton}</Menu.Target>
            {exportMenuItems}
          </Menu>
        </div>

        <Stack gap="sm">
          <IosGroup>
            <MetricsPickerRow
              label="Time range"
              value={dateRange}
              onChange={(value) => isDateRange(value) && setDateRange(value)}
              data={toRangeOptions(data?.ranges)}
            />
            <MetricsPickerRow
              label="Aggregation"
              value={aggregationType}
              onChange={(value) => isAggregationType(value) && setAggregationType(value)}
              data={AGGREGATION_TYPE_OPTIONS}
            />
          </IosGroup>
          <SegmentedControl
            fullWidth
            aria-label="Group by"
            value={aggregationPeriod}
            onChange={(value) => isAggregationPeriod(value) && setAggregationPeriod(value)}
            data={AGGREGATION_PERIOD_OPTIONS}
          />
        </Stack>

        {renderBody()}
      </Stack>
    )
  }

  return (
    <Stack gap={isMobile ? 'xs' : 'md'} pt="xs" pb="md">
      <Group component="section" aria-label="Metrics filters and export" align="flex-start" gap="sm" wrap="wrap">
        <FilterSelect
          label="Time range"
          clearable={false}
          value={dateRange}
          onChange={(value) => isDateRange(value) && setDateRange(value)}
          data={toRangeOptions(data?.ranges)}
        />
        <FilterSelect
          label="Group by"
          clearable={false}
          value={aggregationPeriod}
          onChange={(value) => isAggregationPeriod(value) && setAggregationPeriod(value)}
          data={AGGREGATION_PERIOD_OPTIONS}
        />
        <FilterSelect
          label="Aggregation"
          clearable={false}
          value={aggregationType}
          onChange={(value) => isAggregationType(value) && setAggregationType(value)}
          data={AGGREGATION_TYPE_OPTIONS}
        />
        <Menu position="bottom-end" shadow="md" disabled={!canExport}>
          <Menu.Target>
            <Button
              variant="default"
              size="xs"
              leftSection={<IconDownload size={16} aria-hidden />}
              disabled={!canExport}
              loading={exporting}
            >
              Export
            </Button>
          </Menu.Target>
          {exportMenuItems}
        </Menu>
      </Group>

      {renderBody()}
    </Stack>
  )
}
