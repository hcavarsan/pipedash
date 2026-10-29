import { arrayMove } from '@dnd-kit/sortable'
import { ActionIcon, Group, Tabs, Text, Tooltip } from '@mantine/core'
import {
  IconAdjustments,
  IconArrowDown,
  IconArrowLeft,
  IconArrowRight,
  IconArrowUp,
  IconEdit,
  IconFolderPlus,
  IconPlus,
  IconTrash,
} from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'

import { useProviderCancelItems } from '../../hooks/useCancelActiveRuns'
import { useNow } from '../../hooks/useNow'
import { usePinning } from '../../hooks/usePinning'
import { usePipelineViewQuery } from '../../hooks/useUrlState'
import { usePipelineViews, useReorderPipelineViews } from '../../queries/usePipelineViewQueries'
import { useProviders } from '../../queries/useProvidersQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import type { PipelineView, ProviderSummary } from '../../types'
import { providerTone } from '../../utils/fetchInfo'
import { activeTabId, providerTabId, tabSearch } from '../../utils/providerTabs'
import { canAddSubView, reorderSlots } from '../../utils/viewTree'

import { ToneDot } from '../atoms/ToneDot'

import { type ContextMenuItem, useContextMenu } from '../common/ContextMenu'
import { SortableList } from '../common/SortableList'

import { useConfirmDeleteView } from './PipelineViewModals'

export function useViewMenuItems(
  views: PipelineView[],
  orientation: 'horizontal' | 'vertical'
): (view: PipelineView) => ContextMenuItem[] {
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { mutate: reorder } = useReorderPipelineViews()
  const confirmDelete = useConfirmDeleteView()
  const pinItem = usePinning()
  const movableIds = views.filter((g) => g.capabilities.reorder).map((g) => g.id)
  const [prevLabel, nextLabel, PrevIcon, NextIcon] =
    orientation === 'horizontal'
      ? (['Move left', 'Move right', IconArrowLeft, IconArrowRight] as const)
      : (['Move up', 'Move down', IconArrowUp, IconArrowDown] as const)

  return (view: PipelineView): ContextMenuItem[] => {
    const { rename, edit_rules, reorder: canReorder, delete: canDelete } = view.capabilities
    const siblingIds = views.filter((v) => v.parent === view.parent && v.capabilities.reorder).map((v) => v.id)
    const index = siblingIds.indexOf(view.id)
    const move = (to: number) => reorder(reorderSlots(movableIds, arrayMove(siblingIds, index, to)))
    const items: ContextMenuItem[] = []

    if (rename || edit_rules) {
      items.push({ label: 'Edit view…', icon: <IconEdit size={16} />, onClick: () => openEditor(view.id) })
    }
    if (canAddSubView(views, view)) {
      items.push({
        label: 'New sub-view…',
        icon: <IconFolderPlus size={16} />,
        onClick: () => openEditor(null, [], view.id),
      })
    }
    if (view.kind === 'custom') {
      items.push(pinItem('view', view.id))
    }
    if (canReorder && index >= 0) {
      items.push(
        {
          label: prevLabel,
          icon: <PrevIcon size={16} />,
          disabled: index === 0,
          onClick: () => move(index - 1),
        },
        {
          label: nextLabel,
          icon: <NextIcon size={16} />,
          disabled: index === siblingIds.length - 1,
          onClick: () => move(index + 1),
        }
      )
    }
    if (canDelete) {
      items.push(
        { divider: true },
        { label: 'Delete view', icon: <IconTrash size={16} />, color: 'red', onClick: () => confirmDelete(view) }
      )
    }

    return items
  }
}

export function PipelineViewTabs(): ReactNode {
  const query = usePipelineViewQuery()
  const [, setSearchParams] = useSearchParams()
  const { data: views = [] } = usePipelineViews()
  const { data: providers = [] } = useProviders()
  const now = useNow(30_000)
  const { mutate: reorder } = useReorderPipelineViews()
  const { providerItems, dialog } = useProviderCancelItems(providers)
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const openManage = usePipelineViewStore((s) => s.openManage)
  const contextMenu = useContextMenu()
  const menuItems = useViewMenuItems(views, 'horizontal')

  const leading = views.filter((g) => g.kind === 'all')
  const movable = views.filter((g) => g.capabilities.reorder && g.parent === null)
  const movableIds = views.filter((g) => g.capabilities.reorder).map((g) => g.id)
  const trailing = views.filter((g) => g.kind !== 'all' && !g.capabilities.reorder && g.parent === null)

  const onChange = (id: string | null) => {
    if (id) {
      setSearchParams((prev) => tabSearch(prev, id))
    }
  }

  const providerTab = (provider: ProviderSummary) => (
    <Tabs.Tab
      key={provider.id}
      value={providerTabId(provider.id)}
      className="interactive-surface"
      {...contextMenu(() => providerItems(provider))}
    >
      <Group gap="3xs" wrap="nowrap">
        <ToneDot tone={providerTone(provider, now)} />
        <Text size="sm" span>
          {provider.name}
        </Text>
        <Text size="sm" c="dimmed" span>
          {provider.pipeline_count}
        </Text>
      </Group>
    </Tabs.Tab>
  )

  const tab = (view: PipelineView) => (
    <Tabs.Tab key={view.id} value={view.id} className="interactive-surface" {...contextMenu(() => menuItems(view))}>
      <Group gap="3xs" wrap="nowrap">
        <Text size="sm" span>
          {view.name}
        </Text>
        <Text size="sm" c="dimmed" span>
          {view.count}
        </Text>
      </Group>
    </Tabs.Tab>
  )

  return (
    <Group gap="xs" wrap="nowrap" align="center">
      <Tabs value={activeTabId(views, query)} onChange={onChange} flex={1} miw={0}>
        <Tabs.List style={{ flexWrap: 'nowrap', overflowX: 'auto' }}>
          {leading.map(tab)}
          <SortableList
            items={movable}
            getId={(g) => g.id}
            orientation="horizontal"
            handle={false}
            onReorder={(order) => reorder(reorderSlots(movableIds, order))}
          >
            {(view, drag) => (
              <>
                <div ref={drag.setNodeRef} style={drag.style} {...drag.listeners}>
                  {tab(view)}
                </div>
                {view.kind === 'pinned' && providers.map(providerTab)}
              </>
            )}
          </SortableList>
          {trailing.map(tab)}
        </Tabs.List>
      </Tabs>
      <Tooltip label="New view">
        <ActionIcon variant="subtle" color="gray" aria-label="New view" onClick={() => openEditor(null)}>
          <IconPlus size={16} />
        </ActionIcon>
      </Tooltip>
      <Tooltip label="Manage views">
        <ActionIcon variant="subtle" color="gray" aria-label="Manage views" onClick={openManage}>
          <IconAdjustments size={16} />
        </ActionIcon>
      </Tooltip>
      {dialog}
    </Group>
  )
}
