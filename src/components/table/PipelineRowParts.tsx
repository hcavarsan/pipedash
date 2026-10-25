import { ActionIcon, Group, Loader, Menu, Text, Tooltip } from '@mantine/core'
import { IconChartLine, IconDots, IconPin, IconPinFilled, IconPlayerPlayFilled, IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { Pipeline, PipelineStatus, ProviderSummary } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { getPipelineLabels, getReconcileBlockReason, getStatusLabel } from '../../utils/pipelinePresentation'
import { providerAttention } from '../../utils/providerAttention'
import { StatusBadge } from '../atoms/StatusBadge'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import type { SwipeAction } from '../ios/SwipeActions'

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

export function RepositoryFetchIndicator({ row }: { row: TableRow }) {
  if (!row.hasPendingFetch) {
    return null
  }

  return (
    <Tooltip label="Fetching pipelines…">
      <Loader size="sm" aria-label="Fetching pipelines" />
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

interface WorkflowActionMenuProps extends WorkflowActionsProps {
  provider?: ProviderSummary
  onViewHistory: (pipeline: Pipeline) => void
}

export function WorkflowActionMenu({
  pipeline,
  provider,
  isPinned,
  onTogglePin,
  onTrigger,
  onViewMetrics,
  onViewHistory,
}: WorkflowActionMenuProps) {
  const openNotifications = useNotificationCenterStore((state) => state.open)
  const labels = getPipelineLabels(pipeline.provider_type)
  const blockedReason = getReconcileBlockReason(pipeline)
  const headline = provider ? providerAttention(provider).headline : null

  return (
    <Menu position="bottom">
      <Menu.Target>
        <ActionIcon variant="subtle" color="gray" aria-label={`Actions for ${pipeline.name}`}>
          <IconDots size={22} />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Label>{pipeline.name}</Menu.Label>
        {blockedReason && <Menu.Label>{blockedReason}</Menu.Label>}
        <Menu.Item onClick={() => onViewHistory(pipeline)}>{labels.history}</Menu.Item>
        {onViewMetrics && <Menu.Item onClick={() => onViewMetrics(pipeline)}>View metrics</Menu.Item>}
        <Menu.Item disabled={!!blockedReason} onClick={() => onTrigger(pipeline)}>
          {labels.trigger}
        </Menu.Item>
        <Menu.Item onClick={onTogglePin}>{isPinned ? 'Unpin' : 'Pin'}</Menu.Item>
        {provider && (
          <Menu.Item onClick={() => openNotifications({ type: 'provider', providerId: provider.id })}>
            {provider.name} service status{headline ? ` · ${headline}` : ''}
          </Menu.Item>
        )}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}

export function workflowSwipeActions(
  pipeline: Pipeline,
  isPinned: boolean,
  onTogglePin: () => void,
  onTrigger: (pipeline: Pipeline) => void
): SwipeAction[] {
  const pin: SwipeAction = { key: 'pin', label: isPinned ? 'Unpin' : 'Pin', tone: 'orange', onSelect: onTogglePin }

  if (getReconcileBlockReason(pipeline)) {
    return [pin]
  }

  return [
    pin,
    {
      key: 'trigger',
      label: getPipelineLabels(pipeline.provider_type).trigger,
      tone: 'blue',
      primary: true,
      onSelect: () => onTrigger(pipeline),
    },
  ]
}
