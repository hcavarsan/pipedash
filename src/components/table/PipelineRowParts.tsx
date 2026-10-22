import { ActionIcon, Group, Loader, Text, Tooltip } from '@mantine/core'
import {
  IconAlertTriangle,
  IconChartLine,
  IconPin,
  IconPinFilled,
  IconPlayerPlayFilled,
  IconRefresh,
} from '@tabler/icons-react'
import type { ReactNode } from 'react'

import type { Pipeline, PipelineStatus } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { getPipelineLabels, getReconcileBlockReason, getStatusLabel } from '../../utils/pipelinePresentation'
import { StatusBadge } from '../atoms/StatusBadge'

interface SectionHeadingProps {
  id?: string
  icon: ReactNode
  label: string
  count: number
}

export function SectionHeading({ id, icon, label, count }: SectionHeadingProps) {
  return (
    <Group gap="xs">
      {icon}
      <Text id={id} size="sm" fw={600}>
        {label}
      </Text>
      <Text size="sm" c="dimmed">
        {count}
      </Text>
    </Group>
  )
}

export interface ProviderFetchError {
  id: number
  name: string
  error: string
}

export interface TableRow {
  id: string
  type: 'repository' | 'workflow'
  name: string
  pinKey?: string
  expanded?: boolean
  organization?: string
  pipelineCount?: number
  providerIds?: Set<number>
  providerLabel?: string
  providerErrors?: ProviderFetchError[]
  hasPendingFetch?: boolean
  status?: PipelineStatus | null
  statusLabel?: string
  lastRun?: string | null
  pipeline?: Pipeline
}

export function RowStatus({ row }: { row: TableRow }) {
  if (!row.status) {
    return null
  }

  const prefix = row.type === 'repository' ? 'Most severe workflow status. ' : ''
  const lastActivity = row.lastRun ? formatDateTime(row.lastRun) : 'Never'

  return (
    <Tooltip
      multiline
      maw={480}
      label={
        row.pipeline?.provider_type === 'fluxcd'
          ? String(row.pipeline.metadata?.message ?? `Last activity: ${lastActivity}`)
          : `${prefix}Last activity: ${lastActivity}`
      }
    >
      <span>
        <StatusBadge status={row.status} label={row.pipeline ? getStatusLabel(row.pipeline) : row.statusLabel} />
      </span>
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
  const triggerLabel = `${getPipelineLabels(pipeline.provider_type).trigger} ${pipeline.name}`
  const blockedReason = getReconcileBlockReason(pipeline)

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
      <Tooltip label={blockedReason ?? triggerLabel}>
        <ActionIcon
          variant="subtle"
          color="gray"
          aria-label={triggerLabel}
          aria-disabled={!!blockedReason}
          onClick={() => {
            if (!blockedReason) {
              onTrigger(pipeline)
            }
          }}
        >
          {pipeline.provider_type === 'fluxcd' ? <IconRefresh size={16} /> : <IconPlayerPlayFilled size={16} />}
        </ActionIcon>
      </Tooltip>
    </Group>
  )
}
