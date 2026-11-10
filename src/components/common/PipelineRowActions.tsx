import { ActionIcon, Group, Menu, Tooltip, VisuallyHidden } from '@mantine/core'
import { IconChartLine, IconDots, IconInfoCircle } from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'
import { usePluginPresentation } from '../../queries/usePluginsQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { PinToggle, PipelineActions, WorkflowMenuItem } from '../../types/pipelineRows'
import type { ProviderSummary } from '../../types/providers'
import type { ViewPipeline } from '../../types/views'
import { triggerIcon, workflowMenuItems } from '../../utils/pipelineRowActions'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { RefreshButton } from './RefreshButton'
import { RowMenuItems } from './RowMenuItems'

export function WorkflowActions({
  pipeline,
  actions,
}: {
  pipeline: ViewPipeline
  actions: PipelineActions
}): ReactNode {
  const { onViewMetrics } = actions
  const metricsLabel = `View metrics for ${pipeline.name}`
  const trigger = usePluginPresentation(pipeline.provider_type)?.terminology.trigger ?? 'Trigger'
  const triggerLabel = `${trigger} ${pipeline.name}`
  const blocked = !pipeline.actions.trigger.allowed
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
          {...(blocked ? { 'aria-disabled': true, 'aria-describedby': reasonId, 'data-disabled': true } : {})}
          onClick={() => {
            if (!blocked) {
              actions.onTrigger(pipeline)
            }
          }}
        >
          {triggerIcon(pipeline)}
        </ActionIcon>
      </Tooltip>
      {blocked && pipeline.actions.trigger.reason && (
        <VisuallyHidden id={reasonId}>{pipeline.actions.trigger.reason}</VisuallyHidden>
      )}
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
          <IconDots size={20} aria-hidden />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <RowMenuItems items={items} />
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
          <IconDots size={20} aria-hidden />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Label>{pipeline.name}</Menu.Label>
        <RowMenuItems items={items} />
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
