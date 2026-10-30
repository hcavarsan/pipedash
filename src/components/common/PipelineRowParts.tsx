import { ActionIcon, Group, Loader, Menu, Stack, Text, Tooltip, UnstyledButton, VisuallyHidden } from '@mantine/core'
import {
  IconChartLine,
  IconChevronDown,
  IconChevronRight,
  IconDots,
  IconEye,
  IconEyeOff,
  IconFolderMinus,
  IconFolderOpen,
  IconFolderPlus,
  IconHistory,
  IconInfoCircle,
  IconPin,
  IconPinFilled,
  IconPlayerPlayFilled,
  IconRefresh,
} from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'
import { ToneDot } from '../../features/pipelines/components/ToneDot'
import { useNow } from '../../hooks/useNow'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { StatusKey } from '../../theme/status'
import type { PinKind, Pipeline, PipelineStatus, ProviderSummary, SectionMode, ViewPipeline } from '../../types/domain'
import { dataAge, describeFetch, providerTone, TONE_META, worstTone } from '../../utils/fetchInfo'
import { formatDateTime } from '../../utils/formatDateTime'
import {
  getPipelineLabels,
  getPipelineStatusLabel,
  getTriggerBlockReason,
  pipelineStatusKey,
} from '../../utils/pipelinePresentation'
import { StatusBadge } from '../atoms/StatusBadge'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import type { SwipeAction } from '../ios/swipeAction'
import type { ContextMenuItem } from './ContextMenu'
import { RefreshButton } from './RefreshButton'

type SectionExpansion = { onToggle: () => void; expanded: boolean } | { onToggle?: never; expanded?: never }

type SectionHeadingProps = {
  id?: string | undefined
  icon: ReactNode
  label: string
  info?: ReactNode
  actions?: ReactNode
  count: number
} & SectionExpansion

export function SectionHeading({
  id,
  icon,
  label,
  info,
  actions,
  count,
  expanded,
  onToggle,
}: SectionHeadingProps): ReactNode {
  const content = (
    <Group gap="xs" wrap="nowrap">
      {onToggle &&
        (expanded ? (
          <IconChevronDown size={16} style={{ flexShrink: 0 }} aria-hidden />
        ) : (
          <IconChevronRight size={16} style={{ flexShrink: 0 }} aria-hidden />
        ))}
      {icon}
      <Text id={id} size="sm" fw={600} truncate>
        {label}
      </Text>
      <Text size="sm" c="dimmed">
        {count}
      </Text>
    </Group>
  )

  const heading = onToggle ? (
    <UnstyledButton
      className="item-primary"
      px="2xs"
      py="3xs"
      miw={0}
      maw="100%"
      onClick={onToggle}
      aria-expanded={expanded}
    >
      {content}
    </UnstyledButton>
  ) : (
    content
  )

  return info || actions || onToggle ? (
    <Group
      gap="3xs"
      wrap="nowrap"
      maw="100%"
      bdrs="sm"
      className={onToggle ? 'interactive-surface stretched' : undefined}
      style={{ alignSelf: 'flex-start' }}
    >
      {heading}
      {info}
      {actions}
    </Group>
  ) : (
    heading
  )
}

interface BaseTableRow {
  id: string
  name: string
  depth?: number | undefined
  organization?: string | undefined
  providerIds: Set<number>
  providerLabel?: string | undefined
  lastRun?: string | null | undefined
}

export interface SectionRow extends BaseTableRow {
  type: 'section'
  kind: SectionMode | 'view'
  expanded: boolean
  pipelineCount: number
  hasPendingFetch: boolean
  status: StatusKey | null
  pipelines: ViewPipeline[]
}

export interface WorkflowRow extends BaseTableRow {
  type: 'workflow'
  pipeline: ViewPipeline
  status: PipelineStatus
}

export type TableRow = SectionRow | WorkflowRow

export type SectionMenuTarget = Pick<SectionRow, 'type' | 'id' | 'name' | 'kind' | 'pipelines'>

export function PipelineStatusBadge({
  pipeline,
  provider,
}: {
  pipeline: ViewPipeline
  provider: ProviderSummary | undefined
}): ReactNode {
  const now = useNow(30_000)
  const age = provider && dataAge(provider, now)
  const lastActivity = `Last activity: ${pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never'}`
  const activity = pipeline.resource_state?.message ?? lastActivity
  return (
    <Tooltip
      multiline
      maw={480}
      label={
        <Stack gap={2}>
          <Text size="xs">{activity}</Text>
          {age && <Text size="xs">{age.label}</Text>}
          {age && <Text size="xs">{age.detail}</Text>}
        </Stack>
      }
    >
      <Group component="span" gap={4} wrap="nowrap" display="inline-flex" data-raised>
        <StatusBadge status={pipelineStatusKey(pipeline)} label={getPipelineStatusLabel(pipeline)} />
        {age && age.tone !== 'fresh' && (
          <>
            <ToneDot tone={age.tone} />
            <VisuallyHidden>{age.label}</VisuallyHidden>
          </>
        )}
      </Group>
    </Tooltip>
  )
}

export function RepositoryFetchIndicator({ row }: { row: TableRow }): ReactNode {
  if (row.type !== 'section' || !row.hasPendingFetch) {
    return null
  }

  return (
    <Tooltip label="Fetching pipelines…">
      <Loader size="sm" aria-label="Fetching pipelines" />
    </Tooltip>
  )
}

export function FetchInfo({ providers }: { providers: ProviderSummary[] }): ReactNode {
  const now = useNow(30_000)
  const openNotifications = useNotificationCenterStore((state) => state.open)
  const worst = worstTone(providers.map((provider) => providerTone(provider, now)))

  if (providers.length === 0) {
    return null
  }

  const openProvider = (provider: ProviderSummary) => openNotifications({ type: 'provider', providerId: provider.id })
  const [only] = providers
  const button = (
    <ActionIcon
      variant="transparent"
      color="gray"
      c={worst === 'fresh' ? 'dimmed' : TONE_META[worst].color}
      size="sm"
      aria-label="Fetch info"
      onClick={only && providers.length === 1 ? () => openProvider(only) : undefined}
    >
      <IconInfoCircle size={16} aria-hidden />
    </ActionIcon>
  )
  const icon = (
    <Tooltip
      multiline
      maw={420}
      events={{ hover: true, focus: true, touch: false }}
      label={
        <Stack gap="xs">
          {providers.map((provider) => {
            const info = describeFetch(provider, now)
            const tone = providerTone(provider, now)

            return (
              <Stack key={provider.id} gap={0}>
                <Group gap="xs" wrap="nowrap">
                  <ToneDot tone={tone} />
                  <Text size="xs" fw={600}>
                    {info.name} · {TONE_META[tone].label}
                  </Text>
                </Group>
                <Text size="xs">Last fetch: {info.lastFetch}</Text>
                <Text size="xs">Status: {info.status}</Text>
              </Stack>
            )
          })}
        </Stack>
      }
    >
      {providers.length < 2 ? button : <Menu.Target>{button}</Menu.Target>}
    </Tooltip>
  )

  if (providers.length < 2) {
    return icon
  }

  return (
    <Menu position="bottom-end">
      {icon}
      <Menu.Dropdown>
        {providers.map((provider) => (
          <Menu.Item
            key={provider.id}
            leftSection={<ToneDot tone={providerTone(provider, now)} />}
            onClick={() => openProvider(provider)}
          >
            {provider.name}
          </Menu.Item>
        ))}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}

export function RowFetchInfo({
  row,
  getProvider,
}: {
  row: TableRow
  getProvider: (providerId: number) => ProviderSummary | undefined
}): ReactNode {
  return <FetchInfo providers={Array.from(row.providerIds, getProvider).flatMap((p) => p ?? [])} />
}

export interface PipelineActions {
  onTogglePin: (p: ViewPipeline) => void
  onToggleHidden: (p: ViewPipeline) => void
  onAddToView: (p: ViewPipeline) => void
  onRemoveFromView?: ((p: ViewPipeline) => void) | undefined
  removeLabel?: string | undefined
  onTrigger: (p: Pipeline) => void
  onViewMetrics?: ((p: Pipeline) => void) | undefined
  onViewHistory: (p: Pipeline) => void
  onOpenView: (viewId: string) => void
  onRefreshProviders: (providerIds: number[]) => void
  isRefreshing: (providerIds: number[]) => boolean
  pinItem: (kind: PinKind, ref: string) => WorkflowMenuItem
  cancelItem: (label: string, pipelines: ViewPipeline[]) => WorkflowMenuItem | null
}

export interface PipelineSelection {
  selected: Set<string>
  onToggle: (pipeline: ViewPipeline) => void
}

type WorkflowMenuItem = Extract<ContextMenuItem, { label: string }>

export interface PinToggle {
  pinned: boolean
  label: string
  onToggle: () => void
}

export const workflowPin = (pipeline: ViewPipeline, actions: PipelineActions): PinToggle => ({
  pinned: pipeline.pinned,
  label: pipeline.pinned ? 'Unpin' : 'Pin',
  onToggle: () => actions.onTogglePin(pipeline),
})

export const triggerIcon = (pipeline: Pipeline): ReactNode =>
  pipeline.provider_type === 'fluxcd' ? (
    <IconRefresh size={16} aria-hidden />
  ) : (
    <IconPlayerPlayFilled size={16} aria-hidden />
  )

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
  const items: WorkflowMenuItem[] =
    row.kind === 'repository'
      ? [actions.pinItem('repository', repositoryRef(row.id))]
      : row.kind === 'view'
        ? [
            {
              label: 'Open view',
              icon: <IconFolderOpen size={16} aria-hidden />,
              onClick: () => actions.onOpenView(viewRef(row.id)),
            },
          ]
        : []

  return cancel ? [...items, cancel] : items
}

export function workflowMenuItems(
  pipeline: ViewPipeline,
  actions: PipelineActions,
  pin: PinToggle = workflowPin(pipeline, actions)
): WorkflowMenuItem[] {
  const labels = getPipelineLabels(pipeline.provider_type)
  const { onViewMetrics, onRemoveFromView } = actions
  const items: WorkflowMenuItem[] = [
    {
      label: labels.history,
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

  if (!getTriggerBlockReason(pipeline)) {
    items.push({
      label: labels.trigger,
      icon: triggerIcon(pipeline),
      onClick: () => actions.onTrigger(pipeline),
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

  items.push({
    label: pipeline.hidden ? 'Unhide' : 'Hide',
    icon: pipeline.hidden ? <IconEye size={16} aria-hidden /> : <IconEyeOff size={16} aria-hidden />,
    onClick: () => actions.onToggleHidden(pipeline),
  })

  return items
}

export function WorkflowActions({
  pipeline,
  actions,
}: {
  pipeline: ViewPipeline
  actions: PipelineActions
}): ReactNode {
  const { onViewMetrics } = actions
  const metricsLabel = `View metrics for ${pipeline.name}`
  const triggerLabel = `${getPipelineLabels(pipeline.provider_type).trigger} ${pipeline.name}`
  const blockedReason = getTriggerBlockReason(pipeline)
  const reasonId = useId()

  return (
    <Group gap="xs" wrap="nowrap">
      {onViewMetrics && (
        <Tooltip label={metricsLabel}>
          <ActionIcon aria-label={metricsLabel} onClick={() => onViewMetrics(pipeline)}>
            <IconChartLine size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
      )}
      <Tooltip label={triggerLabel}>
        <ActionIcon
          aria-label={triggerLabel}
          {...(blockedReason ? { 'aria-disabled': true, 'aria-describedby': reasonId, 'data-disabled': true } : {})}
          onClick={() => {
            if (!blockedReason) {
              actions.onTrigger(pipeline)
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

export function RowActionMenu({ name, items }: { name: string; items: WorkflowMenuItem[] }): ReactNode {
  if (items.length === 0) {
    return null
  }

  return (
    <Menu position="bottom">
      <Menu.Target>
        <ActionIcon data-row-menu aria-label={`Actions for ${name}`}>
          <IconDots size={22} aria-hidden />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        {items.map((item) => (
          <Menu.Item
            key={item.label}
            leftSection={item.icon}
            {...(item.color === undefined ? {} : { color: item.color })}
            disabled={item.disabled ?? false}
            onClick={item.onClick}
          >
            {item.label}
          </Menu.Item>
        ))}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}

export function SectionActions({
  name,
  providerIds,
  items,
  actions,
}: {
  name: string
  providerIds: number[]
  items: WorkflowMenuItem[]
  actions: PipelineActions
}): ReactNode {
  const refreshLabel = `Refresh ${name}`

  return (
    <Group gap="xs" wrap="nowrap">
      {providerIds.length > 0 && (
        <Tooltip label={refreshLabel}>
          <RefreshButton
            iconSize={16}
            aria-label={refreshLabel}
            pending={actions.isRefreshing(providerIds)}
            onClick={() => actions.onRefreshProviders(providerIds)}
          />
        </Tooltip>
      )}
      <RowActionMenu name={name} items={items} />
    </Group>
  )
}

export function WorkflowActionMenu({
  pipeline,
  provider,
  actions,
  pin,
}: {
  pipeline: ViewPipeline
  provider?: ProviderSummary | undefined
  actions: PipelineActions
  pin?: PinToggle | undefined
}): ReactNode {
  const openNotifications = useNotificationCenterStore((state) => state.open)
  const items = workflowMenuItems(pipeline, actions, pin)

  return (
    <Menu position="bottom">
      <Menu.Target>
        <ActionIcon data-row-menu aria-label={`Actions for ${pipeline.name}`}>
          <IconDots size={22} aria-hidden />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Label>{pipeline.name}</Menu.Label>
        {items.map((action) => (
          <Menu.Item
            key={action.label}
            leftSection={action.icon}
            disabled={action.disabled ?? false}
            onClick={action.onClick}
          >
            {action.label}
          </Menu.Item>
        ))}
        {provider && (
          <Menu.Item
            leftSection={<IconInfoCircle size={16} aria-hidden />}
            onClick={() => openNotifications({ type: 'provider', providerId: provider.id })}
          >
            Provider status
          </Menu.Item>
        )}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
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

  if (getTriggerBlockReason(pipeline)) {
    return [pinAction]
  }

  return [
    pinAction,
    {
      key: 'trigger',
      label: getPipelineLabels(pipeline.provider_type).trigger,
      tone: 'blue',
      primary: true,
      onSelect: () => actions.onTrigger(pipeline),
    },
  ]
}
