import { Box, Button, Menu } from '@mantine/core'
import { IconDownload } from '@tabler/icons-react'
import { type ReactNode, useRef } from 'react'

import type { ApplicationCopy, MetricResult } from '../../../types/applications'
import type { AggregationPeriod } from '../../../types/metrics'
import { LazyMetricsChart } from '../../metrics/components/LazyMetricsChart'
import { useExportMetrics } from '../../metrics/hooks/useExportMetrics'
import { toMetricsDashboard, UNIT_METRIC_TYPE } from '../lib/metricsDashboard'

interface MetricSeriesProps {
  result: MetricResult
  period: AggregationPeriod
  copy: ApplicationCopy
}

export function MetricSeries({ result, period, copy }: MetricSeriesProps): ReactNode {
  const chartRef = useRef<HTMLDivElement>(null)
  const data = toMetricsDashboard(result, period)
  const exporter = useExportMetrics({
    data,
    metricType: UNIT_METRIC_TYPE[result.unit],
    fileBaseName: `${result.id}_${period}`.replace(/[^a-z0-9]/gi, '_'),
    chartRef,
  })

  return (
    <Box>
      <Box ref={chartRef}>
        <LazyMetricsChart data={data} compact chartTitle={result.label} />
      </Box>
      <Menu position="bottom-start">
        <Menu.Target>
          <Button
            size="compact-xs"
            variant="subtle"
            loading={exporter.isPending}
            leftSection={<IconDownload size={14} aria-hidden />}
          >
            {copy.export}
          </Button>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Item onClick={() => exporter.mutate('png')}>PNG</Menu.Item>
          <Menu.Item onClick={() => exporter.mutate('csv')}>CSV</Menu.Item>
        </Menu.Dropdown>
      </Menu>
    </Box>
  )
}
