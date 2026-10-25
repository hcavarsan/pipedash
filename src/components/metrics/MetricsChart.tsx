import { Badge, Box, Card, Group, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import { type KeyboardEvent, type RefObject, type SyntheticEvent, useRef } from 'react'
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
import type { AggregatedMetrics, AggregationPeriod, MetricType } from '../../types'
import { formatCount } from '../../utils/formatCount'

import classes from './metrics.module.css'
import { formatMetricValue, getMetricStats, hasObservation, METRIC_CONFIG } from './metricsUtils'

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
  tick: { fill: 'var(--mantine-color-dimmed)', fontSize: 13 },
}

const getSelectProps = (onSelect: (() => void) | undefined, label: string, isSelected: boolean) => {
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

const CustomDot = ({ cx, cy, payload, color, onSelect, small = false }: CustomDotProps) => {
  if (typeof cx !== 'number' || typeof cy !== 'number' || !payload || payload.value === null) {
    return null
  }

  const { isSelected, index, label } = payload

  return (
    <g {...getSelectProps(onSelect && (() => onSelect(index)), label, isSelected)}>
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
      <Stack gap={4}>
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
    <Stack gap={4}>
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
  const seriesColor = `var(--mantine-color-${color}-filled)`
  const chartHeight = compact ? (mobileShell ? 120 : 150) : isMobile ? 300 : 450
  const dateFormat = (mobileShell ? SHELL_DATE_FORMATS : DATE_FORMATS)[data.aggregation_period]
  const hoverIndex = useRef<number | null>(null)

  const chartData: ChartPoint[] = data.metrics.map((metric, index) => {
    const date = new Date(metric.timestamp).toLocaleString('en-US', dateFormat)
    const value = hasObservation(metric, metricType) ? Number(metric.value.toFixed(2)) : null

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

  const dot = (props: Omit<CustomDotProps, 'color' | 'onSelect' | 'small'>) => (
    <CustomDot {...props} color={seriesColor} onSelect={onSelect} small={compact} />
  )

  const readIndex = (state: { activeTooltipIndex?: unknown }) =>
    state.activeTooltipIndex === null || state.activeTooltipIndex === undefined
      ? null
      : Number(state.activeTooltipIndex)

  const selectIndex = (index: number | null) => {
    if (index !== null && Number.isInteger(index) && index >= 0 && index < chartData.length) {
      handleSelect(index)
    }
  }

  const handleChartMove = (state: { activeTooltipIndex?: unknown }) => {
    hoverIndex.current = readIndex(state)
  }

  const handleChartClick = (state: { activeTooltipIndex?: unknown }) => {
    const index = readIndex(state)

    if (index === null) {
      window.setTimeout(() => selectIndex(hoverIndex.current), 60)
    } else {
      selectIndex(index)
    }
  }

  const chartProps = {
    data: chartData,
    margin: mobileShell
      ? { top: 8, right: compact ? 8 : 12, left: compact ? 8 : 0, bottom: compact ? 4 : 0 }
      : { top: 5, right: 5, left: 5, bottom: 5 },
    accessibilityLayer: !compact,
    onClick: mobileShell && onSelect ? handleChartClick : undefined,
    onMouseMove: mobileShell && onSelect ? handleChartMove : undefined,
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
    <XAxis dataKey="date" height={28} tickMargin={6} minTickGap={16} interval="preserveStartEnd" {...axisStyle} />
  ) : (
    <XAxis dataKey="date" angle={-45} textAnchor="end" height={80} {...axisStyle} />
  )
  const yAxis = compact ? null : (
    <YAxis
      tickFormatter={(value: number) => formatMetricValue(value, metricType)}
      allowDecimals={metricType !== 'run_frequency'}
      width={mobileShell ? 56 : 80}
      {...axisStyle}
    />
  )
  const tooltip = compact ? null : (
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
                    !!payload?.isSelected
                  )}
                >
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

  const plot = (
    <ResponsiveContainer width="100%" height={chartHeight}>
      {renderChart()}
    </ResponsiveContainer>
  )

  if (compact) {
    return plot
  }

  const stats = getMetricStats(data)

  if (mobileShell) {
    const summary =
      stats.observed === stats.total
        ? formatCount(stats.total, 'period')
        : `${stats.observed} of ${formatCount(stats.total, 'period')} have runs`
    const shellStats = [
      { label: 'Average', value: stats.average },
      { label: 'Latest', value: stats.latest },
      { label: 'Max', value: stats.max },
    ]

    return (
      <Stack gap="md">
        <div className={classes.stats}>
          {shellStats.map(({ label, value }) => (
            <div key={label} className={classes.stat}>
              <div className={classes.caption}>{label}</div>
              <div className={classes.statValue}>{formatMetricValue(value, metricType)}</div>
            </div>
          ))}
        </div>
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
              {selectedPoint ? selectedPoint.label : 'Tap a point to see its value'}
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
