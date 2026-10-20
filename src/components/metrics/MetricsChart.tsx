import { Badge, Card, Group, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import type { KeyboardEvent, RefObject, SyntheticEvent } from 'react'
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

import { useIsMobile } from '../../hooks/useIsMobile'
import type { AggregatedMetrics, AggregationPeriod, MetricType } from '../../types'
import { formatCount } from '../../utils/formatCount'

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

const AXIS_STYLE = {
  axisLine: { stroke: 'var(--mantine-color-default-border)' },
  tickLine: { stroke: 'var(--mantine-color-default-border)' },
  tick: { fill: 'var(--mantine-color-dimmed)', fontSize: 'var(--mantine-font-size-xs)' },
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
  const metricType = data.metric_type
  const { color } = METRIC_CONFIG[metricType]
  const seriesColor = `var(--mantine-color-${color}-filled)`
  const chartHeight = compact ? 150 : isMobile ? 300 : 450

  const chartData: ChartPoint[] = data.metrics.map((metric, index) => {
    const date = new Date(metric.timestamp).toLocaleString('en-US', DATE_FORMATS[data.aggregation_period])
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

  const chartProps = {
    data: chartData,
    margin: { top: 5, right: 5, left: 5, bottom: 5 },
    accessibilityLayer: !compact,
  }
  const grid = compact ? null : (
    <CartesianGrid strokeDasharray="3 3" stroke="var(--mantine-color-default-border)" vertical={false} />
  )
  const xAxis = compact ? null : <XAxis dataKey="date" angle={-45} textAnchor="end" height={80} {...AXIS_STYLE} />
  const yAxis = compact ? null : (
    <YAxis
      tickFormatter={(value: number) => formatMetricValue(value, metricType)}
      allowDecimals={metricType !== 'run_frequency'}
      width={80}
      {...AXIS_STYLE}
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
