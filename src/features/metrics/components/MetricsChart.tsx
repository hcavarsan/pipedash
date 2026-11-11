import { Box, Card, Group, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import { type ReactNode, type RefObject, useRef } from 'react'

import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import type { AggregationPeriod, MetricsDashboard, MetricType } from '../../../types/metrics'
import { formatCount } from '../../../utils/formatCount'
import { useMetricsScrub } from '../hooks/useMetricsScrub'
import { formatMetricValue, formatNumber, METRIC_CONFIG } from '../lib/metricsUtils'
import { MetricsPlot, SHELL_Y_WIDTH } from './MetricsPlot'
import type { ChartPoint } from './metricPlotTypes'
import classes from './metrics.module.css'

export interface MetricsChartProps {
  data: MetricsDashboard
  compact?: boolean
  selectedIndex?: number | null
  onDataPointClick?: (index: number | null) => void
  chartTitle: string
  chartRef?: RefObject<HTMLDivElement | null>
}

const DATE_FORMATS: Record<AggregationPeriod, Intl.DateTimeFormatOptions> = {
  hourly: { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' },
  daily: { month: 'short', day: 'numeric' },
  weekly: { month: 'short', day: 'numeric' },
  monthly: { month: 'short', year: 'numeric' },
}

const SHELL_DATE_FORMATS: Record<AggregationPeriod, Intl.DateTimeFormatOptions> = {
  ...DATE_FORMATS,
  hourly: { month: 'short', day: 'numeric', hour: 'numeric' },
}

const StatCard = ({ label, value, metricType }: { label: string; value: number | null; metricType: MetricType }) => (
  <Card p={{ base: 'xs', sm: 'sm' }}>
    <Stack gap="3xs">
      <Text size="xs" c="dimmed" fw={600} truncate>
        {label}
      </Text>
      <Text fz={{ base: 'sm', sm: 'lg' }} fw={700} truncate>
        {formatMetricValue(value, metricType)}
      </Text>
    </Stack>
  </Card>
)

function getSeriesColor(mobileShell: boolean, metricType: MetricType, color: string): string {
  if (!mobileShell) {
    return `var(--mantine-color-${color}-filled)`
  }
  return metricType === 'success_rate' ? 'var(--status-success)' : 'var(--ios-tint)'
}

function getChartHeight(compact: boolean, mobileShell: boolean, isMobile: boolean): number {
  if (compact) {
    return mobileShell ? 100 : 150
  }
  if (mobileShell) {
    return 240
  }
  return isMobile ? 300 : 450
}

export const MetricsChart = ({
  data,
  compact = false,
  selectedIndex,
  onDataPointClick,
  chartTitle,
  chartRef,
}: MetricsChartProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const { metric_type: metricType, aggregation_period: aggregationPeriod, metrics } = data.data
  const { color } = METRIC_CONFIG[metricType]
  const seriesColor = getSeriesColor(mobileShell, metricType, color)
  const chartHeight = getChartHeight(compact, mobileShell, isMobile)
  const dateFormatter = new Intl.DateTimeFormat(
    undefined,
    (mobileShell ? SHELL_DATE_FORMATS : DATE_FORMATS)[aggregationPeriod]
  )
  const textScale = mobileShell
    ? Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--ios-text-scale')) || 1
    : 1
  const yAxisWidth = mobileShell ? Math.round(SHELL_Y_WIDTH * textScale) : 80
  const plotRef = useRef<HTMLDivElement>(null)

  const points: ChartPoint[] = metrics.map((metric, index) => {
    const date = dateFormatter.format(new Date(metric.timestamp))
    const value = data.observed[index] ? Math.round(metric.value * 100) / 100 : null

    return {
      date,
      value,
      index,
      isSelected: selectedIndex === index,
      label: `${date}: ${formatMetricValue(value, metricType)}`,
    }
  })

  const onSelect =
    !compact && onDataPointClick
      ? (index: number) => onDataPointClick(selectedIndex === index ? null : index)
      : undefined
  const selectedPoint = selectedIndex === null || selectedIndex === undefined ? undefined : points[selectedIndex]
  const scrubbable = mobileShell && !compact && Boolean(onDataPointClick)

  useMetricsScrub({
    plotRef,
    enabled: scrubbable,
    pointCount: points.length,
    band: metricType === 'run_frequency',
    yAxisWidth,
    selectedIndex,
    onDataPointClick,
  })

  const chart = (
    <MetricsPlot
      points={points}
      metricType={metricType}
      seriesColor={seriesColor}
      compact={compact}
      mobileShell={mobileShell}
      chartHeight={chartHeight}
      yAxisWidth={yAxisWidth}
      textScale={textScale}
      onSelect={onSelect}
    />
  )
  const plot = scrubbable ? (
    <div ref={plotRef} className={classes.plot}>
      {chart}
    </div>
  ) : (
    chart
  )

  if (compact) {
    return plot
  }

  const { stats } = data

  if (mobileShell) {
    const summary =
      stats.observed === stats.total
        ? formatCount(stats.total, 'period')
        : `${formatNumber(stats.observed)} of ${formatCount(stats.total, 'period')} have runs`
    const shellStats = [
      { label: 'Average', value: stats.average },
      { label: 'Latest', value: stats.latest },
      { label: 'Max', value: stats.max },
    ]

    return (
      <Stack gap="lg">
        <dl className={classes.stats}>
          {shellStats.map(({ label, value }) => (
            <div key={label} className={classes.stat}>
              <dt>{label}</dt>
              <dd className={classes.statValue}>{formatMetricValue(value, metricType)}</dd>
            </div>
          ))}
        </dl>
        <div className={classes.tile} ref={chartRef}>
          <div className={classes.tileTitle}>{chartTitle}</div>
          <div className={classes.caption}>{summary}</div>
          <Box mt="sm">{plot}</Box>
          {onSelect && (
            <div
              className={classes.readout}
              aria-live="polite"
              style={selectedPoint ? { color: seriesColor } : undefined}
            >
              {selectedPoint ? selectedPoint.label : 'Touch the chart and slide to read values'}
            </div>
          )}
        </div>
      </Stack>
    )
  }

  let observation = formatCount(stats.total, 'period')
  if (stats.observed !== stats.total) {
    observation = `${stats.observed} of ${formatCount(stats.total, 'period')} have runs`
  }

  return (
    <Stack gap={isMobile ? 'xs' : 'md'}>
      <SimpleGrid cols={3} spacing={isMobile ? 'xs' : 'sm'}>
        <StatCard label="Average" value={stats.average} metricType={metricType} />
        <StatCard label="Latest" value={stats.latest} metricType={metricType} />
        <StatCard label="Max" value={stats.max} metricType={metricType} />
      </SimpleGrid>

      <Paper p="md" ref={chartRef}>
        <Group mb="md" justify="space-between" align="flex-start" wrap="nowrap">
          <div>
            <Text fw={600}>{chartTitle}</Text>
            <Text size="xs" c="dimmed">
              {observation}
            </Text>
          </div>
          {selectedPoint && (
            <Text size="sm" c="dimmed">
              {selectedPoint.label}
            </Text>
          )}
        </Group>
        {plot}
      </Paper>
    </Stack>
  )
}
