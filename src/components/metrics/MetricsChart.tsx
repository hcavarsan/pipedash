import { Badge, Box, Card, Group, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import { type KeyboardEvent, type RefObject, type SyntheticEvent, useEffect, useRef } from 'react'
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { haptic } from '../../lib/haptics'
import { trackTouches } from '../../lib/touchTracker'
import { SPACING_PX } from '../../theme'
import type { AggregatedMetrics, AggregationPeriod, MetricType } from '../../types'
import { formatCount } from '../../utils/formatCount'

import { nearestIndex, type PlotSpan } from './chartScrub'
import classes from './metrics.module.css'
import { formatMetricValue, formatNumber, getMetricStats, hasObservation, METRIC_CONFIG } from './metricsUtils'

interface MetricsChartProps {
  data: AggregatedMetrics
  compact?: boolean
  selectedIndex?: number | null
  onDataPointClick?: (index: number | null) => void
  chartTitle: string
  chartRef?: RefObject<HTMLDivElement | null>
}

interface ChartPoint {
  date: string
  value: number | null
  index: number
  isSelected: boolean
  label: string
}

interface CustomTooltipProps {
  label?: string | number
  payload?: readonly { payload?: ChartPoint }[]
  metricType: MetricType
}

interface CustomDotProps {
  cx?: number
  cy?: number
  payload?: ChartPoint
  color: string
  onSelect?: (index: number) => void
  small?: boolean
  shell?: boolean
}

interface BarShapeProps {
  fill?: string
  x?: number
  y?: number
  width?: number
  height?: number
  payload?: ChartPoint
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

const AXIS_STYLE = {
  axisLine: { stroke: 'var(--mantine-color-default-border)' },
  tickLine: { stroke: 'var(--mantine-color-default-border)' },
  tick: { fill: 'var(--mantine-color-dimmed)', fontSize: 'var(--mantine-font-size-xs)' },
}

const SHELL_AXIS_STYLE = {
  axisLine: { stroke: 'var(--ios-separator)' },
  tickLine: false,
  tick: { fill: 'var(--ios-secondary-label)', fontSize: 'var(--ios-fs-footnote)' },
}

const SHELL_MARGIN = { top: 8, right: 12, left: 0, bottom: 0 }
const SHELL_Y_WIDTH = 56
const SHELL_X_HEIGHT = 28

interface Scrub {
  span: PlotSpan
  last: number | null
}

const plotSpan = (plot: HTMLElement, yAxisWidth: number): PlotSpan => {
  const rect = plot.getBoundingClientRect()
  const left = SHELL_MARGIN.left + yAxisWidth

  return { left: rect.left + left, width: rect.width - left - SHELL_MARGIN.right }
}

const getSelectProps = (onSelect: (() => void) | undefined, label: string, isSelected: boolean, shell: boolean) => {
  if (!onSelect) {
    return {}
  }

  const handleSelect = (event: SyntheticEvent) => {
    event.stopPropagation()
    onSelect()
  }

  return {
    role: 'button',
    tabIndex: 0,
    'aria-label': label,
    'aria-pressed': isSelected,
    className: shell ? classes.point : classes.desktopPoint,
    onClick: handleSelect,
    onKeyDown: (event: KeyboardEvent) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault()
        handleSelect(event)
      }
    },
    style: { cursor: 'pointer' },
  }
}

const CustomDot = ({ cx, cy, payload, color, onSelect, small = false, shell = false }: CustomDotProps) => {
  if (typeof cx !== 'number' || typeof cy !== 'number' || !payload || payload.value === null) {
    return null
  }

  const { isSelected, index, label } = payload

  return (
    <g {...getSelectProps(onSelect && (() => onSelect(index)), label, isSelected, shell)}>
      {onSelect && <circle className={shell ? classes.ring : classes.desktopRing} cx={cx} cy={cy} r={15} fill="none" />}
      {isSelected && <circle cx={cx} cy={cy} r={10} fill={color} fillOpacity={0.2} stroke="none" />}
      <circle
        cx={cx}
        cy={cy}
        r={isSelected ? 6 : small ? 2 : 4}
        fill={isSelected ? color : 'var(--mantine-color-body)'}
        stroke={color}
        strokeWidth={isSelected ? 3 : 2}
      />
    </g>
  )
}

const CustomTooltip = ({ label, payload, metricType }: CustomTooltipProps) => {
  const point = payload?.[0]?.payload

  if (!point) {
    return null
  }

  return (
    <Paper p="xs" withBorder shadow="md" miw={150}>
      <Stack gap="3xs">
        <Text size="xs" fw={600}>
          {String(label)}
        </Text>
        <Text size="sm" fw={500}>
          {point.value === null ? 'No runs in this period' : formatMetricValue(point.value, metricType)}
        </Text>
      </Stack>
    </Paper>
  )
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
}: MetricsChartProps) => {
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
  const scrubbed = useRef(false)
  const band = metricType === 'run_frequency'

  const chartData: ChartPoint[] = data.metrics.map((metric, index) => {
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

  const handleSelect = (index: number) => onDataPointClick?.(selectedIndex === index ? null : index)
  const onSelect = !compact && onDataPointClick ? handleSelect : undefined
  const selectedPoint = selectedIndex === null || selectedIndex === undefined ? undefined : chartData[selectedIndex]
  const scrubbable = mobileShell && !compact && Boolean(onDataPointClick)
  const pointCount = chartData.length
  const selectedRef = useRef(selectedIndex)

  useEffect(() => {
    selectedRef.current = selectedIndex
  }, [selectedIndex])

  useEffect(() => {
    const plot = plotRef.current

    if (!scrubbable || !plot) {
      return
    }

    const handleClick = (event: globalThis.MouseEvent) => {
      if (scrubbed.current) {
        scrubbed.current = false

        return
      }

      if (event.target instanceof Element && event.target.closest('[role="button"]')) {
        return
      }

      const index = nearestIndex(event.clientX, plotSpan(plot, yAxisWidth), pointCount, band)

      if (index !== null) {
        onDataPointClick?.(selectedRef.current === index ? null : index)
      }
    }

    plot.addEventListener('click', handleClick)

    const untrack = trackTouches<Scrub>(plot, {
      axis: 'x',
      begin: () => {
        scrubbed.current = false

        return { span: plotSpan(plot, yAxisWidth), last: null }
      },
      move: (scrub, drag) => {
        const index = nearestIndex(drag.startX + drag.dx, scrub.span, pointCount, band)

        if (index === null || index === scrub.last) {
          return
        }

        scrub.last = index
        scrubbed.current = true
        haptic('selection')
        onDataPointClick?.(index)
      },
      end: () => undefined,
    })

    return () => {
      plot.removeEventListener('click', handleClick)
      untrack()
    }
  }, [scrubbable, pointCount, band, yAxisWidth, onDataPointClick])

  const dot = (props: Omit<CustomDotProps, 'color' | 'onSelect' | 'small' | 'shell'>) => (
    <CustomDot {...props} color={seriesColor} onSelect={onSelect} small={compact} shell={mobileShell} />
  )

  const chartProps = {
    data: chartData,
    margin: mobileShell
      ? compact
        ? { top: 8, right: 8, left: 8, bottom: 4 }
        : SHELL_MARGIN
      : { top: SPACING_PX['3xs'], right: SPACING_PX['3xs'], left: SPACING_PX['3xs'], bottom: SPACING_PX['3xs'] },
    accessibilityLayer: !compact,
  }
  const axisStyle = mobileShell ? SHELL_AXIS_STYLE : AXIS_STYLE
  const grid = compact ? null : (
    <CartesianGrid
      strokeDasharray="3 3"
      stroke={mobileShell ? 'var(--ios-separator)' : 'var(--mantine-color-default-border)'}
      vertical={false}
    />
  )
  const xAxis = compact ? null : mobileShell ? (
    <XAxis
      dataKey="date"
      height={Math.round(SHELL_X_HEIGHT * textScale)}
      tickMargin={6}
      minTickGap={16}
      interval="preserveStartEnd"
      {...axisStyle}
    />
  ) : (
    <XAxis dataKey="date" angle={-45} textAnchor="end" height={80} {...axisStyle} />
  )
  const yAxis = compact ? null : (
    <YAxis
      tickFormatter={(value: number) => formatMetricValue(value, metricType)}
      allowDecimals={metricType !== 'run_frequency'}
      width={yAxisWidth}
      {...axisStyle}
    />
  )
  const tooltip =
    compact || mobileShell ? null : (
      <Tooltip
        content={({ label, payload }) => <CustomTooltip label={label} payload={payload} metricType={metricType} />}
        cursor={false}
        filterNull={false}
        isAnimationActive={false}
      />
    )

  const renderChart = () => {
    switch (metricType) {
      case 'run_duration':
        return (
          <LineChart {...chartProps}>
            {grid}
            {xAxis}
            {yAxis}
            {tooltip}
            <Line
              type="monotone"
              dataKey="value"
              stroke={seriesColor}
              strokeWidth={2.5}
              connectNulls={false}
              dot={dot}
              activeDot={false}
              isAnimationActive={false}
            />
          </LineChart>
        )
      case 'success_rate':
        return (
          <AreaChart {...chartProps}>
            {grid}
            {xAxis}
            {yAxis}
            {tooltip}
            <Area
              type="monotone"
              dataKey="value"
              stroke={seriesColor}
              fill={seriesColor}
              fillOpacity={0.3}
              strokeWidth={2.5}
              connectNulls={false}
              dot={dot}
              activeDot={false}
              isAnimationActive={false}
            />
          </AreaChart>
        )
      case 'run_frequency':
        return (
          <BarChart {...chartProps}>
            {grid}
            {xAxis}
            {yAxis}
            {tooltip}
            <Bar
              dataKey="value"
              fill={seriesColor}
              isAnimationActive={false}
              shape={({ fill, x, y, width, height, payload }: BarShapeProps) => (
                <g
                  {...getSelectProps(
                    onSelect && payload && (() => onSelect(payload.index)),
                    payload?.label ?? '',
                    !!payload?.isSelected,
                    mobileShell
                  )}
                >
                  {onSelect && (
                    <rect
                      className={mobileShell ? classes.ring : classes.desktopRing}
                      x={(x ?? 0) - 3}
                      y={(y ?? 0) - 3}
                      width={(width ?? 0) + 6}
                      height={(height ?? 0) + 6}
                      rx={7}
                      ry={7}
                      fill="none"
                    />
                  )}
                  <rect
                    x={x}
                    y={y}
                    width={width}
                    height={height}
                    fill={fill}
                    fillOpacity={payload?.isSelected ? 1 : 0.8}
                    stroke={payload?.isSelected ? 'var(--mantine-color-text)' : 'none'}
                    strokeWidth={payload?.isSelected ? 2 : 0}
                    rx={4}
                    ry={4}
                  />
                </g>
              )}
            />
          </BarChart>
        )
    }
  }

  const chart = (
    <ResponsiveContainer width="100%" height={chartHeight}>
      {renderChart()}
    </ResponsiveContainer>
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
