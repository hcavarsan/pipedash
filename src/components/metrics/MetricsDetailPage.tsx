import { Button, Group, Loader, Menu, SegmentedControl, Stack, UnstyledButton } from '@mantine/core'
import { IconDownload, IconFileTypeCsv, IconPhoto } from '@tabler/icons-react'
import { toBlob } from 'html-to-image'
import { useEffect, useRef, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useAggregatedMetrics } from '../../queries/useMetricsQueries'
import type { AggregationPeriod, AggregationType, MetricType } from '../../types'
import { type DateRange, isDateRange } from '../../utils/dateRange'
import { displayErrorNotification, displaySuccessNotification } from '../../utils/errorDisplay'
import { formatDateTime } from '../../utils/formatDateTime'
import { saveFile } from '../../utils/saveFile'
import { ErrorState } from '../common/ErrorState'
import { FilterSelect } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup } from '../ios/IosList'

import { MetricsChart } from './MetricsChart'
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
  refreshTrigger?: number | undefined
}

const VALUE_UNITS: Record<MetricType, string> = {
  run_duration: 'seconds',
  success_rate: '%',
  run_frequency: 'runs',
}

export const MetricsDetailPage = ({
  pipelineId,
  pipelineName,
  repository,
  metricType,
  initialDateRange,
  initialAggregation,
  retentionDays,
  refreshTrigger,
}: MetricsDetailPageProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const [dateRange, setDateRange] = useState<DateRange>(initialDateRange)
  const [aggregationPeriod, setAggregationPeriod] = useState<AggregationPeriod>(initialAggregation)
  const [aggregationType, setAggregationType] = useState<AggregationType>(getDefaultAggregationType(metricType))
  const [selectedDataPointIndex, setSelectedDataPointIndex] = useState<number | null>(null)
  const [exporting, setExporting] = useState<'png' | 'csv' | null>(null)
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

  useEffect(() => {
    if (refreshTrigger !== undefined && refreshTrigger > 0) {
      refetch()
    }
  }, [refreshTrigger, refetch])

  const [prevData, setPrevData] = useState(data)

  if (prevData !== data) {
    setPrevData(data)
    setSelectedDataPointIndex(null)
  }

  const hasData = !!data && data.metrics.length > 0
  const canExport = hasData && !metricsQuery.isFetching && exporting === null
  const metricLabel = METRIC_CONFIG[metricType].label.toLowerCase()
  const fileBaseName = `${pipelineName.replace(/[^a-z0-9]/gi, '_')}_${metricType}_${dateRange}`

  const handleExport = async (format: 'png' | 'csv') => {
    if (!canExport || !data) {
      return
    }

    setExporting(format)
    try {
      let content: string | Uint8Array<ArrayBuffer>

      if (format === 'png') {
        if (!chartRef.current) {
          throw new Error('The chart is not ready to export.')
        }
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

      const saved = await saveFile(`${fileBaseName}.${format}`, content, {
        name: format === 'png' ? 'PNG Image' : 'CSV File',
        extension: format,
        mimeType: format === 'png' ? 'image/png' : 'text/csv',
      })

      if (saved) {
        displaySuccessNotification(
          format === 'png' ? 'Chart exported as PNG' : 'Data exported as CSV',
          format === 'png' ? 'Chart exported' : 'Data exported'
        )
      }
    } catch (error) {
      displayErrorNotification(error, `Couldn't export ${format.toUpperCase()}`)
    } finally {
      setExporting(null)
    }
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
        <MetricsChart
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
      <Menu.Item leftSection={<IconPhoto size={16} />} onClick={() => handleExport('png')}>
        Export as PNG
      </Menu.Item>
      <Menu.Item leftSection={<IconFileTypeCsv size={16} />} onClick={() => handleExport('csv')}>
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
                aria-busy={exporting !== null}
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
              leftSection={<IconDownload size={16} />}
              disabled={!canExport}
              loading={exporting !== null}
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
