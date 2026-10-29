import { Button } from '@mantine/core'
import { IconChartLine, IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { type DateRange, getDateRangeLabel, getDateRangeStart } from '../../../utils/dateRange'

import { getWidestRange } from '../lib/metricsUtils'

interface MetricsEmptyStateProps {
  range: DateRange
  retentionDays: number
  onRangeChange: (range: DateRange) => void
  onRefresh: () => void
  refreshing: boolean
}

export const MetricsEmptyState = ({
  range,
  retentionDays,
  onRangeChange,
  onRefresh,
  refreshing,
}: MetricsEmptyStateProps): ReactNode => {
  const widest = getWidestRange(retentionDays)
  const canWiden = (getDateRangeStart(widest)?.getTime() ?? 0) < (getDateRangeStart(range)?.getTime() ?? 0)

  return (
    <EmptyState
      icon={<IconChartLine size={24} aria-hidden />}
      title="No runs in this time range"
      description={`No runs were recorded for this pipeline in ${getDateRangeLabel(range).toLowerCase()}. Metrics are collected as runs finish.`}
      action={
        canWiden ? (
          <Button variant="default" size="sm" onClick={() => onRangeChange(widest)}>
            Show {getDateRangeLabel(widest).toLowerCase()}
          </Button>
        ) : (
          <Button
            variant="default"
            size="sm"
            leftSection={<IconRefresh size={16} aria-hidden />}
            loading={refreshing}
            onClick={onRefresh}
          >
            Refresh
          </Button>
        )
      }
    />
  )
}
