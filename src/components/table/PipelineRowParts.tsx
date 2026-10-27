import { ActionIcon, Group, Loader, Menu, Text, Tooltip, VisuallyHidden } from '@mantine/core'
import {
  IconChartLine,
  IconDots,
  IconEye,
  IconEyeOff,
  IconHistory,
  IconPin,
  IconPinFilled,
  IconPlayerPlayFilled,
  IconRefresh,
} from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'

import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { Pipeline, PipelineStatus, ProviderSummary } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { getPipelineLabels, getStatusLabel, getTriggerBlockReason } from '../../utils/pipelinePresentation'
import { providerAttention } from '../../utils/providerAttention'
import { StatusBadge } from '../atoms/StatusBadge'
import type { ContextMenuItem } from '../common/ContextMenu'
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
  isHidden?: boolean
  onToggleHidden?: () => void
  onTrigger: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

type WorkflowMenuItem = Extract<ContextMenuItem, { label: string }>

export const triggerIcon = (pipeline: Pipeline) =>
  pipeline.provider_type === 'fluxcd' ? <IconRefresh size={16} /> : <IconPlayerPlayFilled size={16} />

export function workflowMenuItems({
  pipeline,
  isPinned,
  onTogglePin,
  isHidden = false,
  onToggleHidden,
  onTrigger,
  onViewMetrics,
  onViewHistory,
}: WorkflowActionsProps & { onViewHistory: (pipeline: Pipeline) => void }): WorkflowMenuItem[] {
  const labels = getPipelineLabels(pipeline.provider_type)
  const history: WorkflowMenuItem = {
    label: labels.history,
    icon: <IconHistory size={16} />,
    onClick: () => onViewHistory(pipeline),
  }
  const trigger: WorkflowMenuItem = {
    label: labels.trigger,
    icon: triggerIcon(pipeline),
    disabled: Boolean(getTriggerBlockReason(pipeline)),
    onClick: () => onTrigger(pipeline),
  }
  const pin: WorkflowMenuItem = {
    label: isPinned ? 'Unpin' : 'Pin',
    icon: isPinned ? <IconPinFilled size={16} /> : <IconPin size={16} />,
    onClick: onTogglePin,
  }
  const hide: WorkflowMenuItem[] = onToggleHidden
    ? [
        {
          label: isHidden ? 'Unhide' : 'Hide',
          icon: isHidden ? <IconEye size={16} /> : <IconEyeOff size={16} />,
          onClick: onToggleHidden,
        },
      ]
    : []

  if (!onViewMetrics) {
    return [history, trigger, pin, ...hide]
  }

  return [
    history,
    { label: 'View metrics', icon: <IconChartLine size={16} />, onClick: () => onViewMetrics(pipeline) },
    trigger,
    pin,
    ...hide,
  ]
}

export function WorkflowActions({
  pipeline,
  isPinned,
  onTogglePin,
  isHidden = false,
  onToggleHidden,
  onTrigger,
  onViewMetrics,
}: WorkflowActionsProps) {
  const pinLabel = `${isPinned ? 'Unpin' : 'Pin'} ${pipeline.name}`
  const hideLabel = `${isHidden ? 'Unhide' : 'Hide'} ${pipeline.name}`
  const metricsLabel = `View metrics for ${pipeline.name}`
  const triggerLabel = `${getPipelineLabels(pipeline.provider_type).trigger} ${pipeline.name}`
  const blockedReason = getTriggerBlockReason(pipeline)
  const reasonId = useId()

  return (
    <Group gap="xs" wrap="nowrap">
      <Tooltip label={pinLabel}>
        <ActionIcon variant="subtle" color="gray" aria-label={pinLabel} aria-pressed={isPinned} onClick={onTogglePin}>
          {isPinned ? <IconPinFilled size={16} /> : <IconPin size={16} />}
        </ActionIcon>
      </Tooltip>
      {onToggleHidden && (
        <Tooltip label={hideLabel}>
          <ActionIcon
            variant="subtle"
            color="gray"
            aria-label={hideLabel}
            aria-pressed={isHidden}
            onClick={onToggleHidden}
          >
            {isHidden ? <IconEye size={16} /> : <IconEyeOff size={16} />}
          </ActionIcon>
        </Tooltip>
      )}
      {onViewMetrics && (
        <Tooltip label={metricsLabel}>
          <ActionIcon variant="subtle" color="gray" aria-label={metricsLabel} onClick={() => onViewMetrics(pipeline)}>
            <IconChartLine size={16} />
          </ActionIcon>
        </Tooltip>
      )}
      <Tooltip label={triggerLabel}>
        <ActionIcon
          variant="subtle"
          color="gray"
          aria-label={triggerLabel}
          aria-disabled={blockedReason ? true : undefined}
          aria-describedby={blockedReason ? reasonId : undefined}
          data-disabled={blockedReason ? true : undefined}
          onClick={() => {
            if (!blockedReason) {
              onTrigger(pipeline)
            }
          }}
        >
          {triggerIcon(pipeline)}
        </ActionIcon>
      </Tooltip>
      {blockedReason && <VisuallyHidden id={reasonId}>{blockedReason}</VisuallyHidden>}
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
  isHidden,
  onToggleHidden,
  onTrigger,
  onViewMetrics,
  onViewHistory,
}: WorkflowActionMenuProps) {
  const openNotifications = useNotificationCenterStore((state) => state.open)
  const actions = workflowMenuItems({
    pipeline,
    isPinned,
    onTogglePin,
    isHidden,
    onToggleHidden,
    onTrigger,
    onViewMetrics,
    onViewHistory,
  })
  const blockedReason = getTriggerBlockReason(pipeline)
  const headline = provider ? providerAttention(provider).headline : null

  return (
    <Menu position="bottom">
      <Menu.Target>
        <ActionIcon variant="subtle" color="gray" data-row-menu aria-label={`Actions for ${pipeline.name}`}>
          <IconDots size={22} />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Label>{pipeline.name}</Menu.Label>
        {blockedReason && <Menu.Label>{blockedReason}</Menu.Label>}
        {actions.map((action) => (
          <Menu.Item key={action.label} disabled={action.disabled} onClick={action.onClick}>
            {action.label}
          </Menu.Item>
        ))}
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

  if (getTriggerBlockReason(pipeline)) {
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
