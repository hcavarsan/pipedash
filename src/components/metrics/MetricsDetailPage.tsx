import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react'
import { toPng } from 'html-to-image'

import { Select, SimpleGrid, Stack } from '@mantine/core'
import { save } from '@tauri-apps/plugin-dialog'
import { writeFile, writeTextFile } from '@tauri-apps/plugin-fs'

import { useIsMobile } from '../../hooks/useIsMobile'
import { useAggregatedMetrics } from '../../queries/useMetricsQueries'
import type { AggregatedMetrics, AggregationPeriod, AggregationType, MetricType } from '../../types'
import { displayErrorNotification, displaySuccessNotification } from '../../utils/errorDisplay'
import { formatDuration } from '../../utils/formatDuration'

import { MetricsChart } from './MetricsChart'
import { MetricsDataTable } from './MetricsDataTable'

export interface MetricsDetailPageRef {
  exportPNG: () => Promise<void>
  exportCSV: () => Promise<void>
  canExport: boolean
}

interface MetricsDetailPageProps {
  pipelineId: string
  pipelineName: string
  repository?: string
  metricType: MetricType
  initialDateRange?: string
  initialAggregation?: AggregationPeriod
  initialAggregationType?: AggregationType
  retentionDays?: number
  refreshTrigger?: number
}

const DATE_RANGE_OPTIONS = [
  { value: '24h', label: 'Last 24 Hours' },
  { value: '7d', label: 'Last 7 Days' },
  { value: '30d', label: 'Last 30 Days' },
  { value: '90d', label: 'Last 90 Days' },
]

const AGGREGATION_OPTIONS = [
  { value: 'hourly', label: 'Hourly' },
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'monthly', label: 'Monthly' },
]

const AGGREGATION_TYPE_OPTIONS = [
  { value: 'avg', label: 'Average' },
  { value: 'sum', label: 'Sum' },
  { value: 'min', label: 'Minimum' },
  { value: 'max', label: 'Maximum' },
  { value: 'p95', label: '95th Percentile' },
  { value: 'p99', label: '99th Percentile' },
]

const parseDateRangeToDays = (range: string): number => {
  const value = parseInt(range, 10)

  if (range.endsWith('h')) {
    return value / 24
  }

  return value
}

const buildSmartTitle = (repository: string | undefined, pipelineName: string, metricType: MetricType): string => {
  const metricTitle = getMetricTitle(metricType)

  if (!repository) {
    return `${pipelineName} - ${metricTitle}`
  }

  const repoName = repository.split('/').pop() || repository
  const workflowName = pipelineName

  if (repoName.toLowerCase() === workflowName.toLowerCase()) {
    return `${pipelineName} - ${metricTitle}`
  }

  if (workflowName.toLowerCase().includes(repoName.toLowerCase())) {
    return `${pipelineName} - ${metricTitle}`
  }

  return `${repository} / ${pipelineName} - ${metricTitle}`
}

const getMetricTitle = (type: MetricType): string => {
  switch (type) {
    case 'run_duration':
      return 'Run Duration'
    case 'success_rate':
      return 'Success Rate'
    case 'run_frequency':
      return 'Run Frequency'
  }
}

const getDefaultAggregationType = (metricType: MetricType): AggregationType => {
  switch (metricType) {
    case 'success_rate':
      return 'avg'
    case 'run_duration':
      return 'avg'
    case 'run_frequency':
      return 'sum'
  }
}

export const MetricsDetailPage = forwardRef<MetricsDetailPageRef, MetricsDetailPageProps>(({
  pipelineId,
  pipelineName,
  repository,
  metricType,
  initialDateRange = '24h',
  initialAggregation = 'hourly',
  initialAggregationType,
  retentionDays = 7,
  refreshTrigger,
}, ref) => {
  const { isMobile } = useIsMobile()
  const [dateRange, setDateRange] = useState(initialDateRange)
  const [aggregationPeriod, setAggregationPeriod] = useState<AggregationPeriod>(initialAggregation)
  const [aggregationType, setAggregationType] = useState<AggregationType>(
    initialAggregationType || getDefaultAggregationType(metricType)
  )
  const [selectedDataPointIndex, setSelectedDataPointIndex] = useState<number | null>(null)
  const chartRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    setAggregationType(getDefaultAggregationType(metricType))
  }, [metricType])

  const { startDate, endDate } = useMemo(() => {
    const end = new Date()
    const start = new Date()

    switch (dateRange) {
      case '24h':
        start.setHours(start.getHours() - 24)
        break
      case '7d':
        start.setDate(start.getDate() - 7)
        break
      case '30d':
        start.setDate(start.getDate() - 30)
        break
      case '90d':
        start.setDate(start.getDate() - 90)
        break
    }

    return {
      startDate: start.toISOString(),
      endDate: end.toISOString(),
    }
  }, [dateRange])

  const metricsQuery = useAggregatedMetrics({
    metricType,
    aggregationPeriod,
    aggregationType,
    pipelineId,
    startDate,
    endDate,
  })

  useEffect(() => {
    if (refreshTrigger !== undefined && refreshTrigger > 0) {
      metricsQuery.refetch()
    }
  }, [refreshTrigger, metricsQuery])

  useEffect(() => {
    setSelectedDataPointIndex(null)
  }, [metricsQuery.data])

  const fillMissingPeriods = (metrics: AggregatedMetrics): AggregatedMetrics => {
    if (!metrics || metrics.metrics.length === 0) {
      return metrics
    }

    const result = [...metrics.metrics]
    const period = metrics.aggregation_period

    const getNextPeriod = (timestamp: string): Date => {
      const date = new Date(timestamp)

      switch (period) {
        case 'hourly':
          date.setHours(date.getHours() + 1)
          break
        case 'daily':
          date.setDate(date.getDate() + 1)
          break
        case 'weekly':
          date.setDate(date.getDate() + 7)
          break
        case 'monthly':
          date.setMonth(date.getMonth() + 1)
          break
      }

      return date
    }

    const filled: typeof metrics.metrics = []

    for (let i = 0; i < result.length - 1; i++) {
      filled.push(result[i])

      let current = new Date(result[i].timestamp)
      const next = new Date(result[i + 1].timestamp)

      while (true) {
        const nextPeriod = getNextPeriod(current.toISOString())

        if (nextPeriod >= next) {
          break
        }

        filled.push({
          timestamp: nextPeriod.toISOString(),
          value: 0,
          count: 0,
          min: null,
          max: null,
          avg: 0,
        })
        current = nextPeriod
      }
    }
    filled.push(result[result.length - 1])

    return {
      ...metrics,
      metrics: filled,
      total_count: filled.length,
    }
  }

  const filledData = useMemo(() => {
    return metricsQuery.data ? fillMissingPeriods(metricsQuery.data) : null
  }, [metricsQuery.data])

  const canExport = !!filledData && !metricsQuery.isLoading

  const getAvailableDateRanges = () => {
    return DATE_RANGE_OPTIONS.map((option) => {
      const days = parseDateRangeToDays(option.value)
      const isDisabled = days > retentionDays

      return {
        ...option,
        disabled: isDisabled,
      }
    })
  }

  const getAvailableAggregations = () => {
    return AGGREGATION_OPTIONS
  }

  const handleExportPNG = useCallback(async () => {
    if (!chartRef.current || !filledData) {
      return
    }

    try {
      const computedStyle = window.getComputedStyle(document.body)
      const bgColor = computedStyle.backgroundColor || '#ffffff'

      const dataUrl = await toPng(chartRef.current, {
        quality: 1,
        pixelRatio: 2,
        backgroundColor: bgColor,
        style: {
          backgroundColor: bgColor,
        },
      })

      const base64Data = dataUrl.split(',')[1]
      const binaryData = atob(base64Data)
      const bytes = new Uint8Array(binaryData.length)

      for (let i = 0; i < binaryData.length; i++) {
        bytes[i] = binaryData.charCodeAt(i)
      }

      const fileName = `${pipelineName.replace(/[^a-z0-9]/gi, '_')}_${metricType}_${dateRange}.png`
      const filePath = await save({
        defaultPath: fileName,
        filters: [{ name: 'PNG Image', extensions: ['png'] }],
      })

      if (filePath) {
        await writeFile(filePath, bytes)
        displaySuccessNotification('Chart exported as PNG')
      }
    } catch (error) {
      displayErrorNotification(error, 'Failed to Export PNG')
    }
  }, [filledData, pipelineName, metricType, dateRange])

  const handleExportCSV = useCallback(async () => {
    if (!filledData) {
      return
    }

    try {
      const formatValue = (value: number | null): string => {
        if (value === null || value === undefined) {
          return ''
        }
        if (metricType === 'run_duration') {
          return formatDuration(value)
        }
        if (metricType === 'success_rate') {
          return `${value.toFixed(1)}%`
        }

        return value.toFixed(2)
      }

      const headers = ['Timestamp', 'Date', 'Value', 'Count', 'Min', 'Max', 'Avg']
      const rows = filledData.metrics.map((m) => [
        m.timestamp,
        new Date(m.timestamp).toLocaleString(),
        formatValue(m.value),
        m.count.toString(),
        formatValue(m.min),
        formatValue(m.max),
        formatValue(m.avg),
      ])

      const csvContent = [headers.join(','), ...rows.map((row) => row.join(','))].join('\n')

      const fileName = `${pipelineName.replace(/[^a-z0-9]/gi, '_')}_${metricType}_${dateRange}.csv`
      const filePath = await save({
        defaultPath: fileName,
        filters: [{ name: 'CSV File', extensions: ['csv'] }],
      })

      if (filePath) {
        await writeTextFile(filePath, csvContent)
        displaySuccessNotification('Data exported as CSV')
      }
    } catch (error) {
      displayErrorNotification(error, 'Failed to Export CSV')
    }
  }, [filledData, pipelineName, metricType, dateRange])

  useImperativeHandle(ref, () => ({
    exportPNG: handleExportPNG,
    exportCSV: handleExportCSV,
    canExport,
  }), [canExport, handleExportPNG, handleExportCSV])

  const pageTitle = buildSmartTitle(repository, pipelineName, metricType)

  return (
    <Stack gap={isMobile ? 'xs' : 'md'} pt="xs" pb="md">
      <SimpleGrid cols={isMobile ? 3 : 3} spacing={isMobile ? 'xs' : 'sm'}>
        <Select
          label={isMobile ? undefined : 'Time Range'}
          placeholder={isMobile ? 'Time Range' : undefined}
          value={dateRange}
          onChange={(value) => setDateRange(value || '7d')}
          data={getAvailableDateRanges()}
          size={isMobile ? 'xs' : 'sm'}
        />
        <Select
          label={isMobile ? undefined : 'Group By'}
          placeholder={isMobile ? 'Group By' : undefined}
          value={aggregationPeriod}
          onChange={(value) => setAggregationPeriod((value as AggregationPeriod) || 'daily')}
          data={getAvailableAggregations()}
          size={isMobile ? 'xs' : 'sm'}
        />
        <Select
          label={isMobile ? undefined : 'Aggregation'}
          placeholder={isMobile ? 'Aggregation' : undefined}
          value={aggregationType}
          onChange={(value) => setAggregationType((value as AggregationType) || 'avg')}
          data={AGGREGATION_TYPE_OPTIONS}
          size={isMobile ? 'xs' : 'sm'}
        />
      </SimpleGrid>

      <Stack gap={isMobile ? 'sm' : 'md'}>
        <MetricsChart
          data={filledData}
          loading={metricsQuery.isLoading}
          selectedIndex={selectedDataPointIndex}
          onDataPointClick={setSelectedDataPointIndex}
          chartTitle={pageTitle}
          chartRef={chartRef}
        />
        <MetricsDataTable
          data={filledData}
          selectedIndex={selectedDataPointIndex}
          onRowClick={setSelectedDataPointIndex}
        />
      </Stack>
    </Stack>
  )
})

MetricsDetailPage.displayName = 'MetricsDetailPage'
