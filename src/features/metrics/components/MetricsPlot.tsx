import type { ReactNode } from 'react'
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
import { formatMetricValue } from '../lib/metricsUtils'
import { CustomDot, CustomTooltip, MetricBarShape } from './MetricPlotMarks'
import type { BarShapeProps, CustomDotProps, MetricsPlotProps } from './metricPlotTypes'

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

interface ChartMargin {
  top: number
  right: number
  left: number
  bottom: number
}

export const SHELL_MARGIN: ChartMargin = { top: SPACING_PX.xs, right: SPACING_PX.sm, left: 0, bottom: 0 }
export const SHELL_Y_WIDTH = 56
const SHELL_X_HEIGHT = 28

function getChartMargin(mobileShell: boolean, compact: boolean): ChartMargin {
  if (!mobileShell) {
    return { top: SPACING_PX['3xs'], right: SPACING_PX['3xs'], left: SPACING_PX['3xs'], bottom: SPACING_PX['3xs'] }
  }
  return compact
    ? { top: SPACING_PX.xs, right: SPACING_PX.xs, left: SPACING_PX.xs, bottom: SPACING_PX['3xs'] }
    : SHELL_MARGIN
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

  const barShape = (props: BarShapeProps) => <MetricBarShape {...props} mobileShell={mobileShell} onSelect={onSelect} />

  const chartProps = {
    data: points,
    margin: getChartMargin(mobileShell, compact),
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
  let xAxis: ReactNode = null
  if (!compact) {
    xAxis = mobileShell ? (
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
  }
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
            <Bar dataKey="value" fill={seriesColor} isAnimationActive={false} shape={barShape} />
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
