import { Badge, Box, Card, Group, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import { type ReactNode, type RefObject, useRef } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import type { AggregatedMetrics, AggregationPeriod, MetricType } from '../../types'
import { formatCount } from '../../utils/formatCount'
import { type ChartPoint, MetricsPlot, SHELL_Y_WIDTH } from './MetricsPlot'
import classes from './metrics.module.css'
import { formatMetricValue, formatNumber, getMetricStats, hasObservation, METRIC_CONFIG } from './metricsUtils'
import { useMetricsScrub } from './useMetricsScrub'

export interface MetricsChartProps {
  data: AggregatedMetrics
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
  const metricType = data.metric_type
  const { color } = METRIC_CONFIG[metricType]
  const seriesColor = mobileShell
    ? metricType === 'success_rate'
      ? 'var(--status-success)'
      : 'var(--ios-tint)'
    : `var(--mantine-color-${color}-filled)`
  const chartHeight = compact ? (mobileShell ? 100 : 150) : mobileShell ? 240 : isMobile ? 300 : 450
  const dateFormatter = new Intl.DateTimeFormat(
    undefined,
    (mobileShell ? SHELL_DATE_FORMATS : DATE_FORMATS)[data.aggregation_period]
  )
  const textScale = mobileShell
    ? Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--ios-text-scale')) || 1
    : 1
  const yAxisWidth = mobileShell ? Math.round(SHELL_Y_WIDTH * textScale) : 80
  const plotRef = useRef<HTMLDivElement>(null)

  const points: ChartPoint[] = data.metrics.map((metric, index) => {
    const date = dateFormatter.format(new Date(metric.timestamp))
    const value = hasObservation(metric, metricType) ? Math.round(metric.value * 100) / 100 : null

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

  const stats = getMetricStats(data)

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
              {stats.observed === stats.total
                ? formatCount(stats.total, 'period')
                : `${stats.observed} of ${formatCount(stats.total, 'period')} have runs`}
            </Text>
          </div>
          {selectedPoint && (
            <Badge size="lg" color={color} variant="light">
              {selectedPoint.label}
            </Badge>
          )}
        </Group>
        {plot}
      </Paper>
    </Stack>
  )
}
