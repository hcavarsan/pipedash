import { Paper, Stack, Text } from '@mantine/core'
import type { KeyboardEvent, ReactNode, SyntheticEvent } from 'react'
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

import { SPACING_PX } from '../../../theme/theme'
import type { MetricType } from '../../../types/domain'
import { formatMetricValue } from '../lib/metricsUtils'
import classes from './metrics.module.css'

export interface ChartPoint {
  date: string
  value: number | null
  index: number
  isSelected: boolean
  label: string
}

interface CustomTooltipProps {
  label?: string | number | undefined
  payload?: readonly { payload?: ChartPoint | undefined }[] | undefined
  metricType: MetricType
}

interface CustomDotProps {
  cx?: number | undefined
  cy?: number | undefined
  payload?: ChartPoint | undefined
  color: string
  onSelect?: ((index: number) => void) | undefined
  small?: boolean | undefined
  shell?: boolean | undefined
}

interface BarShapeProps {
  fill?: string | undefined
  x?: number | undefined
  y?: number | undefined
  width?: number | undefined
  height?: number | undefined
  payload?: ChartPoint | undefined
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

export const SHELL_MARGIN = { top: 8, right: 12, left: 0, bottom: 0 }
export const SHELL_Y_WIDTH = 56
const SHELL_X_HEIGHT = 28

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

interface MetricsPlotProps {
  points: ChartPoint[]
  metricType: MetricType
  seriesColor: string
  compact: boolean
  mobileShell: boolean
  chartHeight: number
  yAxisWidth: number
  textScale: number
  onSelect: ((index: number) => void) | undefined
}

export function MetricsPlot({
  points,
  metricType,
  seriesColor,
  compact,
  mobileShell,
  chartHeight,
  yAxisWidth,
  textScale,
  onSelect,
}: MetricsPlotProps): ReactNode {
  const dot = (props: Omit<CustomDotProps, 'color' | 'onSelect' | 'small' | 'shell'>) => (
    <CustomDot {...props} color={seriesColor} onSelect={onSelect} small={compact} shell={mobileShell} />
  )

  const chartProps = {
    data: points,
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

  return (
    <ResponsiveContainer width="100%" height={chartHeight}>
      {renderChart()}
    </ResponsiveContainer>
  )
}
