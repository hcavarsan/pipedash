import { ActionIcon, Button, Group, Stack, Text, Tooltip, UnstyledButton } from '@mantine/core'
import { IconGripVertical, IconPlus, IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { usePipelineViews, useReorderPipelineViews } from '../../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import type { PipelineView } from '../../types'
import { reorderSlots } from '../../utils/viewTree'
import { SortableList } from '../common/SortableList'
import { StandardModal } from '../common/StandardModal'
import { useConfirmDeleteView } from './useConfirmDeleteView'

interface ManageViewListProps {
  views: PipelineView[]
  parentId: string | null
  movableIds: string[]
}

function ManageViewList({ views, parentId, movableIds }: ManageViewListProps): ReactNode {
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { mutate: reorder } = useReorderPipelineViews()
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
        onReorder={(order) => reorder(reorderSlots(movableIds, order))}
      >
        {(view, drag) => (
          <Stack ref={drag.setNodeRef} style={drag.style} gap="4xs" opacity={drag.isDragging ? 0.6 : undefined}>
            <Group gap="xs" wrap="nowrap">
              <ActionIcon
                {...drag.handleProps}
                {...drag.listeners}
                aria-label={`Reorder ${view.name}`}
                style={{ touchAction: 'none', cursor: 'grab' }}
              >
                <IconGripVertical size={16} aria-hidden />
              </ActionIcon>
              <UnstyledButton
                className="interactive-surface"
                flex={1}
                miw={0}
                bdrs="sm"
                px="xs"
                py="2xs"
                onClick={() => openEditor(view.id)}
              >
                <Text size="sm" truncate>
                  {view.name}
                </Text>
              </UnstyledButton>
              {view.capabilities.delete && (
                <Tooltip label={`Delete ${view.name}`}>
                  <ActionIcon aria-label={`Delete ${view.name}`} onClick={() => confirmDelete(view)}>
                    <IconTrash size={16} aria-hidden />
                  </ActionIcon>
                </Tooltip>
              )}
            </Group>
            <ManageViewList views={views} parentId={view.id} movableIds={movableIds} />
          </Stack>
        )}
      </SortableList>
    </Stack>
  )
}

export function ManageViewsModal(): ReactNode {
  const opened = usePipelineViewStore((s) => s.manageOpen)
  const closeManage = usePipelineViewStore((s) => s.closeManage)
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { data: views = [] } = usePipelineViews(opened)
  const movableIds = views.filter((g) => g.capabilities.reorder).map((g) => g.id)
  const configViews = views.filter((g) => g.kind === 'config')

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
      <Stack gap="lg">
        <ManageViewList views={views} parentId={null} movableIds={movableIds} />

        {configViews.length > 0 && (
          <Stack gap="2xs">
            {configViews.map((view) => (
              <Stack key={view.id} gap={0} px="xs">
                <Text size="sm" truncate>
                  {view.name}
                </Text>
                <Text size="xs" c="dimmed">
                  From config.toml
                </Text>
              </Stack>
            ))}
          </Stack>
        )}
      </Stack>
    </StandardModal>
  )
}
