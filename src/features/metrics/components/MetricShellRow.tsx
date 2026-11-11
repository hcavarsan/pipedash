import { IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosRow } from '../../../components/ios/IosList'
import type { AggregatedMetric, MetricType } from '../../../types/metrics'
import { formatDateTime } from '../../../utils/formatDateTime'
import { formatMetricValue, formatNumber } from '../lib/metricsUtils'
import classes from './metrics.module.css'

export interface IndexedMetric {
  metric: AggregatedMetric
  originalIndex: number
  observed: boolean
}

interface MetricShellRowProps {
  row: IndexedMetric
  metricType: MetricType
  selected: boolean
  onSelect: () => void
}

export function MetricShellRow({ row, metricType, selected, onSelect }: MetricShellRowProps): ReactNode {
  const { metric, observed } = row
  const range = `Min ${formatMetricValue(metric.min, metricType)} · Max ${formatMetricValue(metric.max, metricType)}`

  return (
    <IosRow
      title={formatDateTime(metric.timestamp)}
      subtitle={<span className={classes.num}>{`Count ${formatNumber(metric.count)} · ${range}`}</span>}
      value={<span className={classes.num}>{formatMetricValue(observed ? metric.value : null, metricType)}</span>}
      accessory={selected ? <IconCheck size={16} color="var(--ios-tint)" aria-hidden /> : undefined}
      selected={selected}
      onClick={onSelect}
    />
  )
}
