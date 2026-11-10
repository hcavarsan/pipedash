import { ActionIcon, Button, Group, Stack, Text, Tooltip, UnstyledButton } from '@mantine/core'
import { IconFolder, IconGripVertical, IconPlus, IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { StandardModal } from '../../../components/common/StandardModal'
import type { PipelineView } from '../../../types/views'
import { reorderSlots } from '../../../utils/viewTree'
import { useConfirmDeleteView } from '../hooks/useConfirmDeleteView'
import { usePipelineViews } from '../queries/usePipelineViewQueries'
import { useReorderPipelineViews, useViewsReordering } from '../queries/useViewReorderQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { QueryBoundary } from './QueryBoundary'
import { type SortableDragProps, SortableList } from './SortableList'

interface ManageViewRowProps {
  view: PipelineView
  drag: SortableDragProps
  onEdit: () => void
  onDelete: () => void
}

function ManageViewRow({ view, drag, onEdit, onDelete }: ManageViewRowProps): ReactNode {
  return (
    <Group gap="xs" wrap="nowrap">
      <ActionIcon
        {...drag.handleProps}
        {...drag.listeners}
        aria-label={`Reorder ${view.name}`}
        style={{ touchAction: 'none', cursor: 'grab' }}
      >
        <IconGripVertical size={16} aria-hidden />
      </ActionIcon>
      <UnstyledButton className="interactive-surface" flex={1} miw={0} bdrs="sm" px="xs" py="2xs" onClick={onEdit}>
        <Text size="sm" truncate>
          {view.name}
        </Text>
      </UnstyledButton>
      {view.capabilities.delete && (
        <Tooltip label={`Delete ${view.name}`}>
          <ActionIcon aria-label={`Delete ${view.name}`} onClick={onDelete}>
            <IconTrash size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
      )}
    </Group>
  )
}

function ConfigViewItem({ view }: { view: PipelineView }): ReactNode {
  return (
    <Stack gap={0} px="xs">
      <Text size="sm" truncate>
        {view.name}
      </Text>
      <Text size="xs" c="dimmed">
        From config.toml
      </Text>
    </Stack>
  )
}

interface ManageViewListProps {
  views: PipelineView[]
  parentId: string | null
  movableIds: string[]
}

function ManageViewList({ views, parentId, movableIds }: ManageViewListProps): ReactNode {
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { mutate: reorder } = useReorderPipelineViews()
  const { reordering } = useViewsReordering()
  const confirmDelete = useConfirmDeleteView()
  const siblings = views.filter((v) => v.parent === parentId && v.capabilities.reorder)

  if (siblings.length === 0) {
    return null
  }

  return (
    <Stack gap="4xs" {...(parentId === null ? {} : { pl: 'lg' })}>
      <SortableList
        items={siblings}
        getId={(g) => g.id}
        orientation="vertical"
        handle
        disabled={reordering}
        onReorder={(order) => reorder(reorderSlots(movableIds, order))}
      >
        {(view, drag) => (
          <Stack ref={drag.setNodeRef} style={drag.style} gap="4xs" opacity={drag.isDragging ? 0.6 : undefined}>
            <ManageViewRow
              view={view}
              drag={drag}
              onEdit={() => openEditor(view.id)}
              onDelete={() => confirmDelete(view)}
            />
            <ManageViewList views={views} parentId={view.id} movableIds={movableIds} />
          </Stack>
        )}
      </SortableList>
    </Stack>
  )
}

function ManageViewsContent({ views }: { views: PipelineView[] }): ReactNode {
  const movableIds = views.filter((g) => g.capabilities.reorder).map((g) => g.id)
  const configViews = views.filter((g) => g.kind === 'config')

  if (views.length === 0) {
    return (
      <EmptyState
        icon={<IconFolder size={24} aria-hidden />}
        title="No views yet"
        description="Create a view to organize your pipelines your way."
      />
    )
  }

  return (
    <Stack gap="lg">
      <ManageViewList views={views} parentId={null} movableIds={movableIds} />

      {configViews.length > 0 && (
        <Stack gap="2xs">
          {configViews.map((view) => (
            <ConfigViewItem key={view.id} view={view} />
          ))}
        </Stack>
      )}
    </Stack>
  )
}

export function ManageViewsModal(): ReactNode {
  const opened = usePipelineViewStore((s) => s.manageOpen)
  const closeManage = usePipelineViewStore((s) => s.closeManage)
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const viewsQuery = usePipelineViews(opened)

  return (
    <StandardModal
      opened={opened}
      onClose={closeManage}
      size="md"
      title="Manage views"
      footer={
        <Group justify="space-between" gap="sm">
          <Button
            variant="default"
            size="sm"
            leftSection={<IconPlus size={16} aria-hidden />}
            onClick={() => openEditor(null)}
          >
            New view
          </Button>
          <Button size="sm" onClick={closeManage}>
            Done
          </Button>
        </Group>
      }
    >
      <QueryBoundary query={viewsQuery} loading="Loading views…" failure="Couldn't load views">
        {(views) => <ManageViewsContent views={views} />}
      </QueryBoundary>
    </StandardModal>
  )
}
