import type { MetricType } from '../../../types/metrics'

export interface ChartPoint {
  date: string
  value: number | null
  index: number
  isSelected: boolean
  label: string
}

export interface CustomTooltipProps {
  label?: string | number | undefined
  payload?: readonly { payload?: ChartPoint | undefined }[] | undefined
  metricType: MetricType
}

export interface CustomDotProps {
  cx?: number | undefined
  cy?: number | undefined
  payload?: ChartPoint | undefined
  color: string
  onSelect?: ((index: number) => void) | undefined
  small?: boolean | undefined
  shell?: boolean | undefined
}

export interface BarShapeProps {
  fill?: string | undefined
  x?: number | undefined
  y?: number | undefined
  width?: number | undefined
  height?: number | undefined
  payload?: ChartPoint | undefined
}

export interface MetricsPlotProps {
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
