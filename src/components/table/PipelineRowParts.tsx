import { ActionIcon, Group, Loader, Tooltip } from '@mantine/core'
import { IconAlertTriangle, IconChartLine, IconPin, IconPinFilled, IconPlayerPlayFilled } from '@tabler/icons-react'

import type { Pipeline, PipelineStatus } from '../../types'
import { TableCells } from '../../utils/tableCells'

export interface ProviderFetchError {
  id: number
  name: string
  error: string
}

export interface TableRow {
  id: string
  type: 'repository' | 'workflow' | 'pinned-header'
  name: string
  pinned?: boolean
  pinKey?: string
  expanded?: boolean
  organization?: string
  pipelineCount?: number
  providerIds?: Set<number>
  providerLabel?: string
  providerErrors?: ProviderFetchError[]
  hasPendingFetch?: boolean
  status?: PipelineStatus | null
  lastRun?: string | null
  pipeline?: Pipeline
}

export function RowStatus({ row }: { row: TableRow }) {
  if (!row.status) {
    return null
  }

  const prefix = row.type === 'repository' ? 'Most severe workflow status. ' : ''
  const lastActivity = row.lastRun ? new Date(row.lastRun).toLocaleString() : 'Never'

  return (
    <Tooltip label={`${prefix}Last activity: ${lastActivity}`}>
      <div>{TableCells.status(row.status)}</div>
    </Tooltip>
  )
}

interface RepositoryFetchIndicatorProps {
  row: TableRow
  onShowError: (error: ProviderFetchError) => void
}

export function RepositoryFetchIndicator({ row, onShowError }: RepositoryFetchIndicatorProps) {
  if (row.hasPendingFetch) {
    return (
      <Tooltip label="Fetching pipelines…">
        <Loader size="sm" aria-label="Fetching pipelines" />
      </Tooltip>
    )
  }

  const error = row.providerErrors?.[0]

  if (!error) {
    return null
  }

  const label = `View ${error.name} fetch error`

  return (
    <Tooltip label={label}>
      <ActionIcon variant="subtle" color="orange" aria-label={label} onClick={() => onShowError(error)}>
        <IconAlertTriangle size={16} />
      </ActionIcon>
    </Tooltip>
  )
}

interface WorkflowActionsProps {
  pipeline: Pipeline
  isPinned: boolean
  onTogglePin: () => void
  onTrigger: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export function WorkflowActions({ pipeline, isPinned, onTogglePin, onTrigger, onViewMetrics }: WorkflowActionsProps) {
  const pinLabel = `${isPinned ? 'Unpin' : 'Pin'} ${pipeline.name}`
  const metricsLabel = `View metrics for ${pipeline.name}`
  const triggerLabel = `Trigger ${pipeline.name}`

  return (
    <Group gap="xs" wrap="nowrap">
      <Tooltip label={pinLabel}>
        <ActionIcon variant="subtle" color="gray" aria-label={pinLabel} aria-pressed={isPinned} onClick={onTogglePin}>
          {isPinned ? <IconPinFilled size={16} /> : <IconPin size={16} />}
        </ActionIcon>
      </Tooltip>
      {onViewMetrics && (
        <Tooltip label={metricsLabel}>
          <ActionIcon variant="subtle" color="gray" aria-label={metricsLabel} onClick={() => onViewMetrics(pipeline)}>
            <IconChartLine size={16} />
          </ActionIcon>
        </Tooltip>
      )}
      <Tooltip label={triggerLabel}>
        <ActionIcon variant="subtle" color="gray" aria-label={triggerLabel} onClick={() => onTrigger(pipeline)}>
          <IconPlayerPlayFilled size={16} />
        </ActionIcon>
      </Tooltip>
    </Group>
  )
}
