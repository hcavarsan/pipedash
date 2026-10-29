import { Button, Group, Loader, Menu, SegmentedControl, Stack, UnstyledButton } from '@mantine/core'
import { IconDownload, IconFileTypeCsv, IconPhoto } from '@tabler/icons-react'
import { type UseMutationResult, useMutation } from '@tanstack/react-query'
import { type ReactNode, type RefObject, useRef, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useAggregatedMetrics } from '../../queries/useMetricsQueries'
import type { AggregatedMetrics, AggregationPeriod, AggregationType, MetricType } from '../../types'
import { type DateRange, isDateRange } from '../../utils/dateRange'
import { displayErrorNotification, displaySuccessNotification } from '../../utils/errorDisplay'
import { formatDateTime } from '../../utils/formatDateTime'
import { saveFile } from '../../utils/saveFile'
import { ErrorState } from '../common/ErrorState'
import { FilterSelect } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup } from '../ios/IosList'

import { LazyMetricsChart } from './LazyMetricsChart'
import { MetricsDataTable } from './MetricsDataTable'
import { MetricsEmptyState } from './MetricsEmptyState'
import { MetricsPickerRow } from './MetricsPickerRow'
import classes from './metrics.module.css'
import {
  AGGREGATION_PERIOD_OPTIONS,
  AGGREGATION_TYPE_OPTIONS,
  buildChartTitle,
  getDefaultAggregationType,
  getRangeOptions,
  hasObservation,
  isAggregationPeriod,
  isAggregationType,
  METRIC_CONFIG,
  useMetricsWindow,
} from './metricsUtils'

interface MetricsDetailPageProps {
  pipelineId: string
  pipelineName: string
  metricType: MetricType
  repository?: string | undefined
  initialDateRange: DateRange
  initialAggregation: AggregationPeriod
  retentionDays: number
}

type ExportFormat = 'png' | 'csv'

const VALUE_UNITS: Record<MetricType, string> = {
  run_duration: 'seconds',
  success_rate: '%',
  run_frequency: 'runs',
}

interface ExportSource {
  data: AggregatedMetrics | undefined
  metricType: MetricType
  fileBaseName: string
  chartRef: RefObject<HTMLDivElement | null>
}

function useExportMetrics({
  data,
  metricType,
  fileBaseName,
  chartRef,
}: ExportSource): UseMutationResult<boolean, Error, ExportFormat> {
  return useMutation({
    mutationFn: async (format: ExportFormat) => {
      if (!data) {
        return false
      }

      let content: string | Uint8Array<ArrayBuffer>

      if (format === 'png') {
        if (!chartRef.current) {
          throw new Error('The chart is not ready to export.')
        }
        const { toBlob } = await import('html-to-image')
        const backgroundColor = window.getComputedStyle(document.body).backgroundColor
        const image = await toBlob(chartRef.current, { pixelRatio: 2, backgroundColor })

        if (!image) {
          throw new Error('The chart image could not be created.')
        }
        content = new Uint8Array(await image.arrayBuffer())
      } else {
        const cell = (value: number | string | null) => `"${value ?? ''}"`
        const headers = ['Timestamp', 'Date', `Value (${VALUE_UNITS[metricType]})`, 'Count', 'Min', 'Max', 'Avg']
        const rows = data.metrics.map((metric) => [
          metric.timestamp,
          formatDateTime(metric.timestamp),
          hasObservation(metric, metricType) ? metric.value : null,
          metric.count,
          metric.min,
          metric.max,
          hasObservation(metric, metricType) ? metric.avg : null,
        ])

        content = [headers, ...rows].map((row) => row.map(cell).join(',')).join('\n')
      }

      return saveFile(`${fileBaseName}.${format}`, content, {
        name: format === 'png' ? 'PNG Image' : 'CSV File',
        extension: format,
        mimeType: format === 'png' ? 'image/png' : 'text/csv',
      })
    },
    onSuccess: (saved, format) => {
      if (saved) {
        displaySuccessNotification(
          format === 'png' ? 'Chart exported as PNG' : 'Data exported as CSV',
          format === 'png' ? 'Chart exported' : 'Data exported'
        )
      }
    },
    onError: (error, format) => {
      displayErrorNotification(error, `Couldn't export ${format.toUpperCase()}`)
    },
  })
}

export const MetricsDetailPage = ({
  pipelineId,
  pipelineName,
  repository,
  metricType,
  initialDateRange,
  initialAggregation,
  retentionDays,
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

  const { startDate, endDate } = useMetricsWindow(dateRange)

  const metricsQuery = useAggregatedMetrics({
    metricType,
    aggregationPeriod,
    aggregationType,
    pipelineId,
    startDate,
    endDate,
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
  const hasData = !!data && data.metrics.length > 0
  const canExport = hasData && !metricsQuery.isFetching && !exporting
  const metricLabel = METRIC_CONFIG[metricType].label.toLowerCase()

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

    if (!data || data.metrics.length === 0) {
      return (
        <MetricsEmptyState
          range={dateRange}
          retentionDays={retentionDays}
          onRangeChange={setDateRange}
          onRefresh={() => refetch()}
          refreshing={metricsQuery.isFetching}
        />
      )
    }

    return (
      <Stack gap={mobileShell ? 'lg' : isMobile ? 'sm' : 'md'}>
        <LazyMetricsChart
          data={data}
          selectedIndex={selectedDataPointIndex}
          onDataPointClick={setSelectedDataPointIndex}
          chartTitle={buildChartTitle(metricType, aggregationType, aggregationPeriod, dateRange)}
          chartRef={chartRef}
        />
        <MetricsDataTable data={data} selectedIndex={selectedDataPointIndex} onRowClick={setSelectedDataPointIndex} />
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

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <div className={classes.detailHeader}>
          <div className={classes.caption}>{repository ? `${repository} / ${pipelineName}` : pipelineName}</div>
          <Menu position="bottom" disabled={!canExport}>
            <Menu.Target>
              <UnstyledButton
                className={classes.exportButton}
                disabled={!canExport}
                aria-label="Export metrics"
                aria-busy={exporting}
              >
                {exporting ? <Loader size={18} /> : <IconDownload size={18} aria-hidden />}
                {exporting ? 'Exporting…' : 'Export'}
              </UnstyledButton>
            </Menu.Target>
            {exportMenuItems}
          </Menu>
        </div>

        <Stack gap="sm">
          <IosGroup>
            <MetricsPickerRow
              label="Time range"
              value={dateRange}
              onChange={(value) => isDateRange(value) && setDateRange(value)}
              data={getRangeOptions(retentionDays)}
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
          data={getRangeOptions(retentionDays)}
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
