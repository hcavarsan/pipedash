import { Card, Group, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import type { ApplicationCopy, MetricResult } from '../../../types/applications'
import type { AggregationPeriod } from '../../../types/metrics'
import { formatApplicationMetric } from '../lib/metricsDashboard'
import { MetricSeries } from './MetricSeries'

interface MetricCardProps {
  result: MetricResult
  period: AggregationPeriod
  copy: ApplicationCopy
}

function Percentiles({ result }: { result: MetricResult }): ReactNode {
  if (result.p50 === null && result.p95 === null) {
    return null
  }

  return (
    <Group gap="md">
      {result.p50 !== null && (
        <Text size="xs" c="dimmed">
          p50 {formatApplicationMetric(result.p50, result.unit)}
        </Text>
      )}
      {result.p95 !== null && (
        <Text size="xs" c="dimmed">
          p95 {formatApplicationMetric(result.p95, result.unit)}
        </Text>
      )}
    </Group>
  )
}

export function MetricCard({ result, period, copy }: MetricCardProps): ReactNode {
  return (
    <Card withBorder data-metric={result.id}>
      <Stack gap="xs">
        <Text size="xs" c="dimmed" fw={600}>
          {result.label}
        </Text>
        {result.value === null ? (
          <Text size="sm">{result.unavailable_reason}</Text>
        ) : (
          <Text fz="xl" fw={700}>
            {formatApplicationMetric(result.value, result.unit)}
          </Text>
        )}
        <Percentiles result={result} />
        <Text size="xs" c="dimmed">
          {result.coverage.label}
        </Text>
        {result.series.length > 0 && <MetricSeries result={result} period={period} copy={copy} />}
      </Stack>
    </Card>
  )
}
