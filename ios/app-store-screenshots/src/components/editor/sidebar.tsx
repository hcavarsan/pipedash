'use client'
import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import { arrayMove, SortableContext, sortableKeyboardCoordinates, verticalListSortingStrategy } from '@dnd-kit/sortable'
import { Plus } from 'lucide-react'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { newSlide } from '@/lib/defaults'
import { isCreative } from '@/lib/device-specs'
import type { Device, Orientation, Scene, Slide, Theme } from '@/lib/types'
import { SlideThumb } from './slide-thumb'

type Props = {
  slides: Slide[]
  activeId: string | null
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  appName?: string | undefined
  appIcon?: string | undefined
  fontFamily?: string | undefined
  connectedCanvas: boolean
  scene?: Scene | undefined
  disabled?: boolean | undefined
  onReorder: (next: Slide[]) => void
  onSelect: (id: string) => void
  onDelete: (id: string) => void
  onDuplicate: (id: string) => void
  onAdd: (slide: Slide) => void
}

type ListProps = Omit<Props, 'disabled' | 'onAdd'>

function defaultLayoutFor(device: Device): Slide['layout'] {
  if (device === 'feature-graphic') {
    return 'feature-graphic'
  }
  if (device === 'mac' || device.endsWith('-landscape') || isCreative(device)) {
    return 'split-landscape'
  }
  return 'device-bottom'
}

function EmptySlideList(): ReactNode {
  return (
    <div className="rounded-lg border border-dashed p-6 text-center">
      <p className="text-xs font-medium text-foreground">No screens yet</p>
      <p className="mt-1 text-[11px] text-muted-foreground">
        Click <span className="font-semibold">Add screen</span> to get started.
      </p>
    </div>
  )
}

function SlideList({
  slides,
  activeId,
  device,
  orientation,
  theme,
  locale,
  appName,
  appIcon,
  fontFamily,
  connectedCanvas,
  scene,
  onReorder,
  onSelect,
  onDelete,
  onDuplicate,
}: ListProps): ReactNode {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates })
  )

  const handleDragEnd = (e: DragEndEvent) => {
    const { active, over } = e
    if (!over || active.id === over.id) {
      return
    }
    const oldIdx = slides.findIndex((s) => s.id === active.id)
    const newIdx = slides.findIndex((s) => s.id === over.id)
    if (oldIdx === -1 || newIdx === -1) {
      return
    }
    onReorder(arrayMove(slides, oldIdx, newIdx))
  }

  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
      <SortableContext items={slides.map((s) => s.id)} strategy={verticalListSortingStrategy}>
        <div className="space-y-1.5">
          {slides.map((slide, i) => (
            <SlideThumb
              key={slide.id}
              slide={slide}
              slides={slides}
              index={i}
              active={slide.id === activeId}
              device={device}
              orientation={orientation}
              theme={theme}
              locale={locale}
              appName={appName}
              appIcon={appIcon}
              fontFamily={fontFamily}
              connectedCanvas={connectedCanvas}
              scene={scene}
              onSelect={() => onSelect(slide.id)}
              onDelete={() => onDelete(slide.id)}
              onDuplicate={() => onDuplicate(slide.id)}
            />
          ))}
          {slides.length === 0 && <EmptySlideList />}
        </div>
      </SortableContext>
    </DndContext>
  )
}

export function Sidebar({ disabled, onAdd, ...listProps }: Props): ReactNode {
  const { slides, device } = listProps
  const handleAdd = () => onAdd(newSlide(defaultLayoutFor(device)))

  return (
    <div className="flex h-full flex-col">
      <div className="border-b p-3">
        <h2 className="text-sm font-semibold">Screens</h2>
        <p className="text-xs text-muted-foreground">
          {slides.length} screen{slides.length === 1 ? '' : 's'} · drag to reorder
        </p>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        <SlideList {...listProps} />
      </div>

      <div className="border-t bg-card p-3">
        <Button type="button" className="w-full" variant="default" onClick={handleAdd} disabled={disabled}>
          <Plus className="h-4 w-4" /> Add screen
        </Button>
      </div>
    </div>
  )
}
