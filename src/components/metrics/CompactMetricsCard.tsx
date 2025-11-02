import { ActionIcon, Card, Grid, Group, Skeleton, Stack, Text, Tooltip } from '@mantine/core'
import { IconActivity, IconClock, IconMaximize } from '@tabler/icons-react'

import type { AggregatedMetrics, MetricType } from '../../types'
import { formatDuration } from '../../utils/formatDuration'

import { MetricsChart } from './MetricsChart'

interface CompactMetricsCardProps {
  metricType: MetricType
  data: AggregatedMetrics | null
  loading?: boolean
  onExpand: () => void
}

export const CompactMetricsCard = ({ metricType, data, loading, onExpand }: CompactMetricsCardProps) => {
  const getMetricConfig = (type: MetricType) => {
    switch (type) {
      case 'run_duration':
        return {
          title: 'Run Duration',
          icon: IconClock,
          color: 'blue',
        }
      case 'success_rate':
        return {
          title: 'Success Rate',
          icon: IconActivity,
          color: 'green',
        }
      case 'run_frequency':
        return {
          title: 'Run Frequency',
          icon: IconActivity,
          color: 'violet',
        }
    }
  }

  const config = getMetricConfig(metricType)

  const calculateKeyStats = () => {
    if (!data || data.metrics.length === 0) {
      return { latest: null, avg: null, trend: null }
    }

    const values = data.metrics.map((m) => m.value)
    const sum = values.reduce((acc, val) => acc + val, 0)
    const avg = sum / values.length
    const latest = values[values.length - 1]
    const previous = values[values.length - 2]
    const trend = previous ? ((latest - previous) / previous) * 100 : null

    return { latest, avg, trend }
  }

  const stats = calculateKeyStats()

  const formatValue = (value: number | null, type: MetricType): string => {
    if (value === null) {
return '-'
}

    if (type === 'run_duration') {
      return formatDuration(value)
    }
    if (type === 'success_rate') {
      return `${value.toFixed(1)}%`
    }

return value.toFixed(0)
  }

  return (
    <Card
      p="md"
      withBorder
      style={{
        cursor: 'pointer',
        transition: 'all 0.2s ease',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.transform = 'translateY(-2px)'
        e.currentTarget.style.boxShadow = '0 4px 12px rgba(0, 0, 0, 0.1)'
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.transform = 'translateY(0)'
        e.currentTarget.style.boxShadow = ''
      }}
      onClick={onExpand}
    >
      <Stack gap="md">
        <Group justify="space-between" align="flex-start">
          <Group gap="xs">
            <config.icon size={20} color={`var(--mantine-color-${config.color}-6)`} />
            <Text size="sm" fw={600} c="dimmed">
              {config.title}
            </Text>
          </Group>
          <Tooltip label="Expand for details" withArrow>
            <ActionIcon
              variant="subtle"
              color="gray"
              size="sm"
              onClick={(e) => {
                e.stopPropagation()
                onExpand()
              }}
            >
              <IconMaximize size={16} />
            </ActionIcon>
          </Tooltip>
        </Group>

        {loading ? (
          <Grid gutter="xs">
            <Grid.Col span={6}>
              <Stack gap={4}>
                <Text size="xs" c="dimmed" tt="uppercase">
                  Latest
                </Text>
                <Skeleton height={28} width="80%" />
              </Stack>
            </Grid.Col>
            <Grid.Col span={6}>
              <Stack gap={4}>
                <Text size="xs" c="dimmed" tt="uppercase">
                  Average
                </Text>
                <Skeleton height={28} width="80%" />
              </Stack>
            </Grid.Col>
          </Grid>
        ) : data && data.metrics.length > 0 ? (
          <Grid gutter="xs">
            <Grid.Col span={6}>
              <Stack gap={4}>
                <Text size="xs" c="dimmed" tt="uppercase">
                  Latest
                </Text>
                <Text size="lg" fw={700}>
                  {formatValue(stats.latest, metricType)}
                </Text>
              </Stack>
            </Grid.Col>
            <Grid.Col span={6}>
              <Stack gap={4}>
                <Text size="xs" c="dimmed" tt="uppercase">
                  Average
                </Text>
                <Text size="lg" fw={700}>
                  {formatValue(stats.avg, metricType)}
                </Text>
              </Stack>
            </Grid.Col>
          </Grid>
        ) : (
          <Stack gap={4} align="center" py="xs">
            <Text size="xs" c="dimmed">
              No data yet
            </Text>
          </Stack>
        )}

        <MetricsChart data={data} loading={loading} compact />
      </Stack>
    </Card>
  )
}
