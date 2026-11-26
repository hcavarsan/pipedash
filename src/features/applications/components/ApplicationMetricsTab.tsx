import { Group, Select, SimpleGrid, Stack, Text, Title } from '@mantine/core'
import { type ReactNode, useMemo, useState } from 'react'

import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import type { ApplicationCopy } from '../../../types/applications'
import type { AggregationPeriod } from '../../../types/metrics'
import { DATE_RANGE_OPTIONS, type DateRange, getDateRangeStart, isDateRange } from '../../../utils/dateRange'
import { AGGREGATION_PERIOD_OPTIONS, isAggregationPeriod } from '../../metrics/lib/metricsUtils'
import { useApplicationMetrics } from '../queries/useApplicationQueries'
import { LazyFunnelPlot } from './LazyFunnelPlot'
import { MetricCard } from './MetricCard'

interface ApplicationMetricsTabProps {
  applicationId: number
  copy: ApplicationCopy
}

const ALL_TARGETS = ''

export function ApplicationMetricsTab({ applicationId, copy }: ApplicationMetricsTabProps): ReactNode {
  const [dateRange, setDateRange] = useState<DateRange>('7d')
  const [period, setPeriod] = useState<AggregationPeriod>('daily')
  const [group, setGroup] = useState(ALL_TARGETS)
  const bounds = useMemo(() => {
    const now = new Date()

    return { from: (getDateRangeStart(dateRange, now) ?? now).toISOString(), to: now.toISOString() }
  }, [dateRange])
  const query = useApplicationMetrics({
    application_id: applicationId,
    group_key: group === ALL_TARGETS ? null : group,
    ...bounds,
    period,
  })

  if (query.isError && !query.data) {
    return <ErrorState title="Couldn't load metrics" error={query.error} onRetry={() => void query.refetch()} />
  }

  if (!query.data) {
    return <LoadingState message="Loading metrics…" />
  }

  const { metrics, funnel, groups } = query.data
  const groupOptions = [
    { value: ALL_TARGETS, label: copy.all_targets },
    ...groups.map((g) => ({ value: g.key, label: g.label })),
  ]

  return (
    <Stack gap="md">
      <Group gap="sm" wrap="wrap">
        <Select
          label="Time range"
          data={DATE_RANGE_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
          value={dateRange}
          onChange={(value) => isDateRange(value) && setDateRange(value)}
          allowDeselect={false}
          w={200}
        />
        <Select
          label="Group by"
          data={AGGREGATION_PERIOD_OPTIONS}
          value={period}
          onChange={(value) => isAggregationPeriod(value) && setPeriod(value)}
          allowDeselect={false}
          w={140}
        />
        <Select
          label={copy.targets}
          data={groupOptions}
          value={group}
          onChange={(value) => setGroup(value ?? ALL_TARGETS)}
          allowDeselect={false}
          w={220}
        />
      </Group>
      {metrics.length === 0 ? (
        <Text c="dimmed" ta="center" py="xl" role="status">
          {copy.metrics_empty}
        </Text>
      ) : (
        <SimpleGrid cols={{ base: 1, sm: 2, lg: 3 }}>
          {metrics.map((result) => (
            <MetricCard key={result.id} result={result} period={period} copy={copy} />
          ))}
        </SimpleGrid>
      )}
      {funnel.length > 0 && (
        <Stack gap="xs">
          <Title order={3} size="h5">
            {copy.funnel}
          </Title>
          <LazyFunnelPlot steps={funnel} />
        </Stack>
      )}
    </Stack>
  )
}
