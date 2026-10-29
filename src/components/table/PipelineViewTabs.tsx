import { ActionIcon, Group, Tabs, Text, Tooltip } from '@mantine/core'
import { IconAdjustments, IconPlus } from '@tabler/icons-react'
import { Fragment, lazy, type ReactNode, Suspense } from 'react'
import { useSearchParams } from 'react-router-dom'

import { useProviderCancelItems } from '../../hooks/useCancelActiveRuns'
import { useNow } from '../../hooks/useNow'
import { usePipelineViewQuery } from '../../hooks/useUrlState'
import { usePipelineViews, useReorderPipelineViews } from '../../queries/usePipelineViewQueries'
import { useProviders } from '../../queries/useProvidersQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import type { PipelineView, ProviderSummary } from '../../types'
import { providerTone } from '../../utils/fetchInfo'
import { activeTabId, providerTabId, tabSearch } from '../../utils/providerTabs'

import { ToneDot } from '../atoms/ToneDot'

import { useContextMenu } from '../common/ContextMenu'

import { useViewMenuItems } from './useViewMenuItems'

const SortableViewTabs = lazy(() => import('./SortableViewTabs').then((m) => ({ default: m.SortableViewTabs })))

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

  const staticTabs = movable.map((view) => (
    <Fragment key={view.id}>
      {tab(view)}
      {view.kind === 'pinned' && providers.map(providerTab)}
    </Fragment>
  ))

  return (
    <Group gap="xs" wrap="nowrap" align="center">
      <Tabs value={activeTabId(views, query)} onChange={onChange} flex={1} miw={0}>
        <Tabs.List style={{ flexWrap: 'nowrap', overflowX: 'auto' }}>
          {leading.map(tab)}
          <Suspense fallback={staticTabs}>
            <SortableViewTabs
              movable={movable}
              movableIds={movableIds}
              providers={providers}
              renderTab={tab}
              renderProviderTab={providerTab}
              onReorder={(order) => reorder(order)}
            />
          </Suspense>
          {trailing.map(tab)}
        </Tabs.List>
      </Tabs>
      <Tooltip label="New view">
        <ActionIcon variant="subtle" color="gray" aria-label="New view" onClick={() => openEditor(null)}>
          <IconPlus size={16} aria-hidden />
        </ActionIcon>
      </Tooltip>
      <Tooltip label="Manage views">
        <ActionIcon variant="subtle" color="gray" aria-label="Manage views" onClick={openManage}>
          <IconAdjustments size={16} aria-hidden />
        </ActionIcon>
      </Tooltip>
      {dialog}
    </Group>
  )
}
