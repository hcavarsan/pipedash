import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  type DraggableAttributes,
  type DraggableSyntheticListeners,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import {
  arrayMove,
  horizontalListSortingStrategy,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { useReducedMotion } from '@mantine/hooks'
import type { CSSProperties, ReactNode } from 'react'

export interface SortableDragProps {
  setNodeRef: (node: HTMLElement | null) => void
  style: CSSProperties
  attributes: DraggableAttributes
  listeners: DraggableSyntheticListeners
  handleProps?: DraggableAttributes & { ref: (node: HTMLElement | null) => void }
  isDragging: boolean
}

interface SortableListProps<T> {
  items: T[]
  getId: (item: T) => string
  onReorder: (nextIds: string[]) => void
  orientation: 'horizontal' | 'vertical'
  handle: boolean
  children: (item: T, dragProps: SortableDragProps) => ReactNode
}

function SortableItem<T>({
  id,
  item,
  handle,
  children,
}: {
  id: string
  item: T
  handle: boolean
  children: SortableListProps<T>['children']
}) {
  const reducedMotion = useReducedMotion()
  const { setNodeRef, setActivatorNodeRef, transform, transition, attributes, listeners, isDragging } = useSortable({
    id,
  })

  return children(item, {
    setNodeRef,
    style: { transform: CSS.Translate.toString(transform), transition: reducedMotion ? undefined : transition },
    attributes,
    listeners,
    handleProps: handle ? { ref: setActivatorNodeRef, ...attributes } : undefined,
    isDragging,
  })
}

export function SortableList<T>({ items, getId, onReorder, orientation, handle, children }: SortableListProps<T>) {
  const pointer = useSensor(PointerSensor, { activationConstraint: { distance: 6 } })
  const keyboard = useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates })
  const sensors = useSensors(pointer, ...(handle ? [keyboard] : []))
  const ids = items.map(getId)

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) {
      return
    }

    const from = ids.indexOf(String(active.id))
    const to = ids.indexOf(String(over.id))

    if (from < 0 || to < 0) {
      return
    }

    onReorder(arrayMove(ids, from, to))
  }

  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
      <SortableContext
        items={ids}
        strategy={orientation === 'horizontal' ? horizontalListSortingStrategy : verticalListSortingStrategy}
      >
        {items.map((item, index) => (
          <SortableItem key={ids[index]} id={ids[index]} item={item} handle={handle}>
            {children}
          </SortableItem>
        ))}
      </SortableContext>
    </DndContext>
  )
}
