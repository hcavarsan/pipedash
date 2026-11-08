import {
  IconChartLine,
  IconEye,
  IconEyeOff,
  IconFolderMinus,
  IconFolderOpen,
  IconFolderPlus,
  IconHistory,
  IconPin,
  IconPinFilled,
  IconPlayerPlayFilled,
  IconPower,
  IconRefresh,
} from '@tabler/icons-react'
import type { ReactNode } from 'react'
import type { SwipeAction } from '../components/ios/swipeAction'
import type { ActionIcon } from '../types/actions'
import type {
  PinToggle,
  PipelineActions,
  SectionMenuTarget,
  WorkflowMenuItem,
  WorkflowRow,
} from '../types/pipelineRows'
import type { Pipeline } from '../types/pipelines'
import type { ViewPipeline } from '../types/views'

export const workflowPin = (pipeline: ViewPipeline, actions: PipelineActions): PinToggle => ({
  pinned: pipeline.pinned,
  label: pipeline.pinned ? 'Unpin' : 'Pin',
  onToggle: () => actions.onTogglePin(pipeline),
})

const TRIGGER_ICONS: Record<ActionIcon, ReactNode> = {
  play: <IconPlayerPlayFilled size={16} aria-hidden />,
  refresh: <IconRefresh size={16} aria-hidden />,
}

export const triggerIcon = (pipeline: Pick<Pipeline, 'actions'>): ReactNode =>
  TRIGGER_ICONS[pipeline.actions.trigger_icon]

export const historyLabel = (actions: PipelineActions, providerType: string): string =>
  (actions.terminology(providerType)?.history ?? 'history').toLowerCase()

export const repositoryRef = (sectionId: string): string => sectionId.slice('repo:'.length)

export const viewRef = (sectionId: string): string => sectionId.slice('view:'.length)

export const refreshItem = (providerIds: number[], actions: PipelineActions): WorkflowMenuItem => ({
  label: 'Refresh',
  icon: <IconRefresh size={16} aria-hidden />,
  disabled: actions.isRefreshing(providerIds),
  onClick: () => actions.onRefreshProviders(providerIds),
})

export const rowMenuItems = (row: WorkflowRow | SectionMenuTarget, actions: PipelineActions): WorkflowMenuItem[] => {
  if (row.type !== 'section') {
    return []
  }

  const cancel = actions.cancelItem(row.name, row.pipelines)
  const items: WorkflowMenuItem[] = []

  if (row.kind === 'repository') {
    items.push(actions.pinItem('repository', repositoryRef(row.id)))
  }

  if (row.kind === 'view') {
    items.push({
      label: 'Open view',
      icon: <IconFolderOpen size={16} aria-hidden />,
      onClick: () => actions.onOpenView(viewRef(row.id)),
    })
  }

  return cancel ? [...items, cancel] : items
}

export function workflowMenuItems(
  pipeline: ViewPipeline,
  actions: PipelineActions,
  pin: PinToggle = workflowPin(pipeline, actions)
): WorkflowMenuItem[] {
  const terminology = actions.terminology(pipeline.provider_type)
  const { onViewMetrics, onRemoveFromView } = actions
  const items: WorkflowMenuItem[] = [
    {
      label: terminology?.history ?? 'History',
      icon: <IconHistory size={16} aria-hidden />,
      onClick: () => actions.onViewHistory(pipeline),
    },
  ]

  if (onViewMetrics) {
    items.push({
      label: 'View metrics',
      icon: <IconChartLine size={16} aria-hidden />,
      onClick: () => onViewMetrics(pipeline),
    })
  }

  if (pipeline.actions.trigger.allowed) {
    items.push({
      label: terminology?.trigger ?? 'Trigger',
      icon: triggerIcon(pipeline),
      onClick: () => actions.onTrigger(pipeline),
    })
  }

  if (pipeline.actions.enable.allowed) {
    items.push({
      label: 'Enable',
      icon: <IconPower size={16} aria-hidden />,
      onClick: () => actions.onEnable(pipeline),
    })
  }

  items.push(
    refreshItem([pipeline.provider_id], actions),
    {
      label: pin.label,
      icon: pin.pinned ? <IconPinFilled size={16} aria-hidden /> : <IconPin size={16} aria-hidden />,
      onClick: pin.onToggle,
    },
    {
      label: 'Add to view…',
      icon: <IconFolderPlus size={16} aria-hidden />,
      onClick: () => actions.onAddToView(pipeline),
    }
  )

  if (onRemoveFromView) {
    items.push({
      label: actions.removeLabel ?? 'Remove from view',
      icon: <IconFolderMinus size={16} aria-hidden />,
      onClick: () => onRemoveFromView(pipeline),
    })
  }

  items.push(...actions.notifyItems(pipeline), {
    label: pipeline.hidden ? 'Unhide' : 'Hide',
    icon: pipeline.hidden ? <IconEye size={16} aria-hidden /> : <IconEyeOff size={16} aria-hidden />,
    onClick: () => actions.onToggleHidden(pipeline),
  })

  return items
}

export function workflowSwipeActions(
  pipeline: ViewPipeline,
  actions: PipelineActions,
  pin: PinToggle = workflowPin(pipeline, actions)
): SwipeAction[] {
  const pinAction: SwipeAction = {
    key: 'pin',
    label: pin.pinned ? 'Unpin' : 'Pin',
    tone: 'orange',
    onSelect: pin.onToggle,
  }

  if (!pipeline.actions.trigger.allowed) {
    return [pinAction]
  }

  return [
    pinAction,
    {
      key: 'trigger',
      label: actions.terminology(pipeline.provider_type)?.trigger ?? 'Trigger',
      tone: 'blue',
      primary: true,
      onSelect: () => actions.onTrigger(pipeline),
    },
  ]
}
