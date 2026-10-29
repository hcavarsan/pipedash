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

import { useNow } from '../../hooks/useNow'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { PinKind, Pipeline, PipelineStatus, ProviderSummary, SectionMode, ViewPipeline } from '../../types'
import { dataAge, describeFetch, providerTone, TONE_META, worstTone } from '../../utils/fetchInfo'
import { formatDateTime } from '../../utils/formatDateTime'
import { getPipelineLabels, getStatusLabel, getTriggerBlockReason } from '../../utils/pipelinePresentation'
import { providerAttention } from '../../utils/providerAttention'
import { StatusBadge } from '../atoms/StatusBadge'
import { ToneDot } from '../atoms/ToneDot'
import type { ContextMenuItem } from '../common/ContextMenu'
import { RefreshButton } from '../common/RefreshButton'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import type { SwipeAction } from '../ios/SwipeActions'

interface SectionHeadingProps {
  id?: string | undefined
  icon: ReactNode
  label: string
  info?: ReactNode
  actions?: ReactNode
  count: number
  expanded?: boolean | undefined
  onToggle?: (() => void) | undefined
}

export function SectionHeading({ id, icon, label, info, actions, count, expanded, onToggle }: SectionHeadingProps) {
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

export interface TableRow {
  id: string
  type: 'section' | 'workflow'
  name: string
  depth?: number | undefined
  kind?: SectionMode | 'view' | undefined
  expanded?: boolean | undefined
  organization?: string | undefined
  pipelineCount?: number | undefined
  providerIds?: Set<number>
  providerLabel?: string
  hasPendingFetch?: boolean
  status?: PipelineStatus | null
  lastRun?: string | null
  pipeline?: ViewPipeline
  pipelines?: ViewPipeline[]
}

export function PipelineStatusBadge({
  pipeline,
  provider,
}: {
  pipeline: ViewPipeline
  provider: ProviderSummary | undefined
}) {
  const now = useNow(30_000)
  const age = provider && dataAge(provider, now)
  const lastActivity = `Last activity: ${pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never'}`
  const activity =
    pipeline.provider_type === 'fluxcd' ? String(pipeline.metadata?.message ?? lastActivity) : lastActivity
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
        <StatusBadge status={pipeline.status} label={getStatusLabel(pipeline)} />
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

export function FetchInfo({ providers }: { providers: ProviderSummary[] }) {
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
      c={TONE_META[worst].color}
      size="sm"
      aria-label="Fetch info"
      onClick={only && providers.length === 1 ? () => openProvider(only) : undefined}
    >
      <IconInfoCircle size={16} />
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
}) {
  return <FetchInfo providers={Array.from(row.providerIds ?? [], getProvider).flatMap((p) => p ?? [])} />
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

export const triggerIcon = (pipeline: Pipeline) =>
  pipeline.provider_type === 'fluxcd' ? <IconRefresh size={16} /> : <IconPlayerPlayFilled size={16} />

export const repositoryRef = (sectionId: string) => sectionId.slice('repo:'.length)

export const viewRef = (sectionId: string) => sectionId.slice('view:'.length)

export const refreshItem = (providerIds: number[], actions: PipelineActions): WorkflowMenuItem => ({
  label: 'Refresh',
  icon: <IconRefresh size={16} />,
  disabled: actions.isRefreshing(providerIds),
  onClick: () => actions.onRefreshProviders(providerIds),
})

export const rowMenuItems = (row: TableRow, actions: PipelineActions): WorkflowMenuItem[] => {
  if (row.type !== 'section') {
    return []
  }

  const cancel = actions.cancelItem(row.name, row.pipelines ?? [])
  const items: WorkflowMenuItem[] =
    row.kind === 'repository'
      ? [actions.pinItem('repository', repositoryRef(row.id))]
      : row.kind === 'view'
        ? [
            {
              label: 'Open view',
              icon: <IconFolderOpen size={16} />,
              onClick: () => actions.onOpenView(viewRef(row.id)),
            },
          ]
        : []

  return cancel ? [...items, cancel] : items
}

export function workflowMenuItems(pipeline: ViewPipeline, actions: PipelineActions): WorkflowMenuItem[] {
  const labels = getPipelineLabels(pipeline.provider_type)
  const { onViewMetrics, onRemoveFromView } = actions
  const items: WorkflowMenuItem[] = [
    { label: labels.history, icon: <IconHistory size={16} />, onClick: () => actions.onViewHistory(pipeline) },
  ]

  if (onViewMetrics) {
    items.push({ label: 'View metrics', icon: <IconChartLine size={16} />, onClick: () => onViewMetrics(pipeline) })
  }

  items.push(
    {
      label: labels.trigger,
      icon: triggerIcon(pipeline),
      disabled: Boolean(getTriggerBlockReason(pipeline)),
      onClick: () => actions.onTrigger(pipeline),
    },
    refreshItem([pipeline.provider_id], actions),
    {
      label: pipeline.pinned ? 'Unpin' : 'Pin',
      icon: pipeline.pinned ? <IconPinFilled size={16} /> : <IconPin size={16} />,
      onClick: () => actions.onTogglePin(pipeline),
    },
    { label: 'Add to view…', icon: <IconFolderPlus size={16} />, onClick: () => actions.onAddToView(pipeline) }
  )

  if (onRemoveFromView) {
    items.push({
      label: actions.removeLabel ?? 'Remove from view',
      icon: <IconFolderMinus size={16} />,
      onClick: () => onRemoveFromView(pipeline),
    })
  }

  items.push({
    label: pipeline.hidden ? 'Unhide' : 'Hide',
    icon: pipeline.hidden ? <IconEye size={16} /> : <IconEyeOff size={16} />,
    onClick: () => actions.onToggleHidden(pipeline),
  })

  return items
}

export function WorkflowActions({ pipeline, actions }: { pipeline: ViewPipeline; actions: PipelineActions }) {
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
            <IconChartLine size={16} />
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

export function RowActionMenu({ name, items }: { name: string; items: WorkflowMenuItem[] }) {
  if (items.length === 0) {
    return null
  }

  return (
    <Menu position="bottom">
      <Menu.Target>
        <ActionIcon data-row-menu aria-label={`Actions for ${name}`}>
          <IconDots size={22} />
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
}) {
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
}: {
  pipeline: ViewPipeline
  provider?: ProviderSummary | undefined
  actions: PipelineActions
}) {
  const openNotifications = useNotificationCenterStore((state) => state.open)
  const items = workflowMenuItems(pipeline, actions)
  const blockedReason = getTriggerBlockReason(pipeline)
  const headline = provider ? providerAttention(provider).headline : null

  return (
    <Menu position="bottom">
      <Menu.Target>
        <ActionIcon data-row-menu aria-label={`Actions for ${pipeline.name}`}>
          <IconDots size={22} />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Label>{pipeline.name}</Menu.Label>
        {blockedReason && <Menu.Label>{blockedReason}</Menu.Label>}
        {items.map((action) => (
          <Menu.Item key={action.label} disabled={action.disabled ?? false} onClick={action.onClick}>
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

export function workflowSwipeActions(pipeline: ViewPipeline, actions: PipelineActions): SwipeAction[] {
  const pin: SwipeAction = {
    key: 'pin',
    label: pipeline.pinned ? 'Unpin' : 'Pin',
    tone: 'orange',
    onSelect: () => actions.onTogglePin(pipeline),
  }

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
      onSelect: () => actions.onTrigger(pipeline),
    },
  ]
}
