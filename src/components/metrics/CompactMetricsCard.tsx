import { Card, Group, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconMaximize } from '@tabler/icons-react'

import type { AggregatedMetrics, MetricType } from '../../types'
import { EmptyState } from '../common/EmptyState'
import { LoadingState } from '../common/LoadingState'

import { MetricsChart } from './MetricsChart'
import { formatMetricValue, getMetricStats, METRIC_CONFIG } from './metricsUtils'

interface CompactMetricsCardProps {
  metricType: MetricType
  data: AggregatedMetrics | undefined
  title: string
  loading: boolean
  onExpand: () => void
}

const CardStat = ({ label, value }: { label: string; value: string }) => (
  <Stack gap={4}>
    <Text size="xs" c="dimmed">
      {label}
    </Text>
    <Text size="lg" fw={700}>
      {value}
    </Text>
  </Stack>
)

export const CompactMetricsCard = ({ metricType, data, title, loading, onExpand }: CompactMetricsCardProps) => {
  const renderBody = () => {
    if (loading) {
      return <LoadingState message={`Loading ${METRIC_CONFIG[metricType].label.toLowerCase()}...`} />
    }

    if (!data || data.metrics.length === 0) {
      return <EmptyState title="No runs in this time range" description="Widen the time range to include older runs." />
    }

    const stats = getMetricStats(data)

    return (
      <>
        <SimpleGrid cols={2} spacing="xs">
          <CardStat label="Latest" value={formatMetricValue(stats.latest, metricType)} />
          <CardStat label="Average" value={formatMetricValue(stats.average, metricType)} />
        </SimpleGrid>
        <MetricsChart data={data} chartTitle={title} compact />
      </>
    )
  }

  return (
    <Card p={0} h="100%">
      <UnstyledButton
        className="interactive-surface"
        w="100%"
        h="100%"
        p="md"
        ta="left"
        aria-label={`Expand ${title}`}
        onClick={onExpand}
      >
        <Stack gap="md" h="100%" justify="flex-start">
          <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xs">
            <Text size="sm" fw={600} c="dimmed">
              {title}
            </Text>
            <IconMaximize size={16} aria-hidden />
          </Group>
          {renderBody()}
        </Stack>
      </UnstyledButton>
    </Card>
  )
}
