import { Card, Group, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconChevronRight, IconMaximize } from '@tabler/icons-react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { AggregatedMetrics, MetricType } from '../../types'
import { EmptyState } from '../common/EmptyState'
import { LoadingState } from '../common/LoadingState'

import { MetricsChart } from './MetricsChart'
import classes from './metrics.module.css'
import { formatMetricValue, getMetricStats, METRIC_CONFIG } from './metricsUtils'

interface CompactMetricsCardProps {
  metricType: MetricType
  data: AggregatedMetrics | undefined
  title: string
  loading: boolean
  onExpand: () => void
}

const CardStat = ({ label, value, shell }: { label: string; value: string; shell: boolean }) =>
  shell ? (
    <div>
      <div className={classes.caption}>{label}</div>
      <div className={classes.compactValue}>{value}</div>
    </div>
  ) : (
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
  const mobileShell = useMobileShell()

  const renderBody = () => {
    if (loading) {
      return <LoadingState message={`Loading ${METRIC_CONFIG[metricType].label.toLowerCase()}...`} />
    }

    if (!data || data.metrics.length === 0) {
      return <EmptyState title="No runs in this time range" description="Widen the time range to include older runs." />
    }

    const stats = getMetricStats(data)
    const latest = <CardStat label="Latest" value={formatMetricValue(stats.latest, metricType)} shell={mobileShell} />
    const average = (
      <CardStat label="Average" value={formatMetricValue(stats.average, metricType)} shell={mobileShell} />
    )

    return (
      <>
        {mobileShell ? (
          <div className={classes.compactStats}>
            {latest}
            {average}
          </div>
        ) : (
          <SimpleGrid cols={2} spacing="xs">
            {latest}
            {average}
          </SimpleGrid>
        )}
        <MetricsChart data={data} chartTitle={title} compact />
      </>
    )
  }

  if (mobileShell) {
    const stats = data && data.metrics.length > 0 ? getMetricStats(data) : null
    const expandLabel = stats
      ? `Expand ${title}. Latest ${formatMetricValue(stats.latest, metricType)}, average ${formatMetricValue(stats.average, metricType)}`
      : `Expand ${title}`

    return (
      <UnstyledButton className={classes.tile} aria-label={expandLabel} onClick={onExpand}>
        <Stack gap="sm">
          <div className={classes.tileHead}>
            <span className={classes.tileTitle}>{title}</span>
            <IconChevronRight size={18} className={classes.chevron} aria-hidden />
          </div>
          {renderBody()}
        </Stack>
      </UnstyledButton>
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
