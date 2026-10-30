import { Button, Group, Menu, Paper, Text } from '@mantine/core'
import {
  IconEye,
  IconEyeOff,
  IconFolder,
  IconFolderMinus,
  IconFolderPlus,
  IconPin,
  IconPinnedOff,
  IconRefresh,
} from '@tabler/icons-react'
import type { ReactNode } from 'react'
import type { ContextMenuItem } from '../../../components/common/ContextMenu'
import { IosToolbar } from '../../../components/common/IosRunList'
import toolbarClasses from '../../../components/common/IosRunList.module.css'
import type { PipelineActions } from '../../../components/common/PipelineRowParts'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import type { PipelineView, ViewPipeline } from '../../../types/domain'
import { canAddSubView } from '../../../utils/viewTree'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { usePipelineViews, useSetViewMembership } from '../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'

type SelectionAction = Extract<ContextMenuItem, { label: string }>

interface SelectionActionsInput {
  selected: ViewPipeline[]
  view: PipelineView
  allViews: PipelineView[]
  actions: PipelineActions
  finish: () => void
  onRemove: ((ids: string[]) => void) | undefined
}

export function buildSelectionActions({
  selected,
  view,
  allViews,
  actions,
  finish,
  onRemove,
}: SelectionActionsInput): SelectionAction[] {
  const allPinned = selected.length > 0 && selected.every((p) => p.pinned)
  const allHidden = selected.length > 0 && selected.every((p) => p.hidden)
  const selectedIds = selected.map((p) => p.id)
  const cancel = actions.cancelItem(
    selected.length === 1 ? '1 selected pipeline' : `${selected.length} selected pipelines`,
    selected
  )

  return [
    {
      label: allPinned ? 'Unpin' : 'Pin',
      icon: allPinned ? <IconPinnedOff size={14} aria-hidden /> : <IconPin size={14} aria-hidden />,
      onClick: () => {
        for (const p of selected.filter((p) => p.pinned === allPinned)) {
          actions.onTogglePin(p)
        }
      },
    },
    {
      label: allHidden ? 'Unhide' : 'Hide',
      icon: allHidden ? <IconEye size={14} aria-hidden /> : <IconEyeOff size={14} aria-hidden />,
      onClick: () => {
        for (const p of selected.filter((p) => p.hidden === allHidden)) {
          actions.onToggleHidden(p)
        }
        finish()
      },
    },
    {
      label: 'Refresh',
      icon: <IconRefresh size={14} aria-hidden />,
      onClick: () => actions.onRefreshProviders([...new Set(selected.map((p) => p.provider_id))]),
    },
    ...(cancel ? [cancel] : []),
    {
      label: 'New view',
      icon: <IconFolderPlus size={14} aria-hidden />,
      onClick: () =>
        usePipelineViewStore.getState().openEditor(null, selectedIds, canAddSubView(allViews, view) ? view.id : null),
    },
    {
      label: 'Add to view…',
      icon: <IconFolder size={14} aria-hidden />,
      onClick: () => usePipelineViewStore.getState().openAddToView(selectedIds),
    },
    ...(onRemove
      ? [
          {
            label: `Remove from ${view.name}`,
            icon: <IconFolderMinus size={14} aria-hidden />,
            color: 'red' as const,
            onClick: () => {
              onRemove(selectedIds)
              finish()
            },
          },
        ]
      : []),
  ]
}

interface SelectionControlsProps {
  model: PipelineViewPageModel
  actions: PipelineActions
  view: PipelineView
}

function SelectionControls({ model, actions, view }: SelectionControlsProps): ReactNode {
  const { selectedPipelines, finishSelection, labels, mobileShell } = model
  const { data: allViews = [] } = usePipelineViews()
  const { mutate: setMembership } = useSetViewMembership()
  const items = buildSelectionActions({
    selected: selectedPipelines,
    view,
    allViews,
    actions,
    finish: finishSelection,
    onRemove:
      view.kind === 'custom' && view.capabilities.edit_members
        ? (ids) => setMembership({ id: view.id, pipelineIds: ids, member: false })
        : undefined,
  })
  const count = selectedPipelines.length
  const status = count === 0 ? `Select ${labels.items}` : `${count} selected`

  if (mobileShell) {
    return (
      <IosToolbar label="Selected pipelines">
        <Button variant="subtle" onClick={finishSelection}>
          Done
        </Button>
        <span className={toolbarClasses.count} role="status">
          {status}
        </span>
        <Menu position="top">
          <Menu.Target>
            <Button variant="subtle" disabled={count === 0}>
              Actions
            </Button>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Label>{`${count} selected`}</Menu.Label>
            {items.map((item) => (
              <Menu.Item
                key={item.label}
                leftSection={item.icon}
                {...(item.color === undefined ? {} : { color: item.color })}
                onClick={item.onClick}
              >
                {item.label}
              </Menu.Item>
            ))}
            <ActionSheetCancel />
          </Menu.Dropdown>
        </Menu>
      </IosToolbar>
    )
  }

  return (
    <Paper withBorder radius="md" px="sm" py="xs">
      <Group justify="space-between" gap="sm" wrap="nowrap" align="flex-start">
        <Text size="sm" fw={600} py={4} style={{ whiteSpace: 'nowrap' }} role="status">
          {status}
        </Text>
        <Group gap="xs" justify="flex-end">
          {items.map((item) => (
            <Button
              key={item.label}
              size="xs"
              variant="light"
              {...(item.color === undefined ? {} : { color: item.color })}
              leftSection={item.icon}
              disabled={count === 0}
              onClick={item.onClick}
            >
              {item.label}
            </Button>
          ))}
        </Group>
      </Group>
    </Paper>
  )
}

export function PipelineViewSelection({
  model,
  actions,
}: {
  model: PipelineViewPageModel
  actions: PipelineActions
}): ReactNode {
  if (!model.selecting || !model.view) {
    return null
  }

  return <SelectionControls model={model} actions={actions} view={model.view} />
}
