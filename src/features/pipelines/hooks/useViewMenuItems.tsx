import {
  IconArrowDown,
  IconArrowLeft,
  IconArrowRight,
  IconArrowUp,
  IconEdit,
  IconFolderPlus,
  IconTrash,
} from '@tabler/icons-react'
import type { ContextMenuItem } from '../../../components/common/ContextMenu'
import type { PipelineView } from '../../../types/domain'
import { canAddSubView, reorderSlots } from '../../../utils/viewTree'
import { useReorderPipelineViews } from '../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { useConfirmDeleteView } from './useConfirmDeleteView'
import { usePinning } from './usePinning'

export function moveId(ids: string[], from: number, to: number): string[] {
  const next = [...ids]
  const [moved] = next.splice(from, 1)
  if (moved !== undefined) {
    next.splice(to, 0, moved)
  }

  return next
}

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
    const move = (to: number) => reorder(reorderSlots(movableIds, moveId(siblingIds, index, to)))
    const items: ContextMenuItem[] = []

    if (rename || edit_rules) {
      items.push({ label: 'Edit view…', icon: <IconEdit size={16} aria-hidden />, onClick: () => openEditor(view.id) })
    }
    if (canAddSubView(views, view)) {
      items.push({
        label: 'New sub-view…',
        icon: <IconFolderPlus size={16} aria-hidden />,
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
          icon: <PrevIcon size={16} aria-hidden />,
          disabled: index === 0,
          onClick: () => move(index - 1),
        },
        {
          label: nextLabel,
          icon: <NextIcon size={16} aria-hidden />,
          disabled: index === siblingIds.length - 1,
          onClick: () => move(index + 1),
        }
      )
    }
    if (canDelete) {
      items.push(
        { divider: true },
        {
          label: 'Delete view',
          icon: <IconTrash size={16} aria-hidden />,
          color: 'red',
          onClick: () => confirmDelete(view),
        }
      )
    }

    return items
  }
}
