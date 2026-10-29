import { Button, Card, Group, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconChevronRight, IconMaximize } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { AggregatedMetrics, MetricType } from '../../types'
import { EmptyState } from '../common/EmptyState'
import { LoadingState } from '../common/LoadingState'

import { LazyMetricsChart } from './LazyMetricsChart'
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
    <Stack gap="3xs">
      <Text size="xs" c="dimmed">
        {label}
      </Text>
      <Text size="lg" fw={700}>
        {value}
      </Text>
    </Stack>
  )

export const CompactMetricsCard = ({
  metricType,
  data,
  title,
  loading,
  onExpand,
}: CompactMetricsCardProps): ReactNode => {
  const mobileShell = useMobileShell()

  const hasData = !loading && !!data && data.metrics.length > 0

  const renderSummary = () => {
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

    return mobileShell ? (
      <div className={classes.compactStats}>
        {latest}
        {average}
      </div>
    ) : (
      <SimpleGrid cols={2} spacing="xs">
        {latest}
        {average}
      </SimpleGrid>
    )
  }

  const chart =
    hasData && data ? (
      <div className={`${classes.aboveOverlay} ${classes.passThrough}`}>
        <LazyMetricsChart data={data} chartTitle={title} compact />
      </div>
    ) : null

  if (mobileShell) {
    const stats = data && data.metrics.length > 0 ? getMetricStats(data) : null
    const expandLabel = stats
      ? `Expand ${title}. Latest ${formatMetricValue(stats.latest, metricType)}, average ${formatMetricValue(stats.average, metricType)}`
      : `Expand ${title}`

    return (
      <div className={`${classes.tile} ${classes.compactCard}`}>
        <Stack gap="sm">
          <UnstyledButton className={classes.expandButton} aria-label={expandLabel} onClick={onExpand}>
            <Stack gap="sm">
              <div className={classes.tileHead}>
                <span className={classes.tileTitle}>{title}</span>
                <IconChevronRight size={18} className={classes.chevron} aria-hidden />
              </div>
              {renderSummary()}
            </Stack>
          </UnstyledButton>
          {chart}
        </Stack>
      </div>
    )
  }

  return (
    <Card p={0} h="100%" pos="relative" className="interactive-surface">
      <Stack gap="md" p="md" h="100%" justify="flex-start">
        <UnstyledButton className={classes.expandButton} aria-label={`Expand ${title}`} onClick={onExpand}>
          <Stack gap="md">
            <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xs">
              <Text size="sm" fw={600} c="dimmed">
                {title}
              </Text>
              <IconMaximize size={16} aria-hidden />
            </Group>
            {renderSummary()}
          </Stack>
        </UnstyledButton>
        {chart}
      </Stack>
      {!loading && !hasData && (
        <Group px="md" pb="md" className={classes.aboveOverlay}>
          <Button variant="default" size="sm" onClick={onExpand}>
            Change time range
          </Button>
        </Group>
      )}
    </Card>
  )
}
