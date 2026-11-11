import { ImagePlus, Plus } from 'lucide-react'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import type { Device, ElementId, Orientation, Patch, Slide } from '@/lib/types'
import { ActiveElementPanel } from './active-element-panel'
import { useElementTransformControls } from './use-element-transform-controls'

export function ElementTransformControls({
  slide,
  defaultTextColor,
  device,
  orientation,
  locale,
  selectedElementId,
  onChange,
  onUpdate,
  onSelectElement,
}: {
  slide: Slide
  defaultTextColor: string
  device: Device
  orientation: Orientation
  locale: string
  selectedElementId: ElementId | null
  onChange: (patch: Patch<Slide>) => void
  onUpdate: (update: (slide: Slide) => Patch<Slide>) => void
  onSelectElement: (id: ElementId | null) => void
}): ReactNode {
  const controls = useElementTransformControls({
    slide,
    device,
    orientation,
    locale,
    selectedElementId,
    onChange,
    onUpdate,
    onSelectElement,
  })
  const { activeId } = controls

  return (
    <div className="space-y-3 rounded-md border bg-muted/30 p-3">
      <div className="space-y-1">
        <div className="flex items-center justify-between gap-2">
          <Label className="text-xs font-semibold">Elements</Label>
          <div className="flex items-center gap-1.5">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 shrink-0 px-2 text-xs"
              onClick={controls.addTextElement}
            >
              <Plus className="h-3.5 w-3.5" />
              Text
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 shrink-0 px-2 text-xs"
              onClick={controls.addImageElement}
              title="Add a PNG or JPG overlay (logo, photo, badge)"
            >
              <ImagePlus className="h-3.5 w-3.5" />
              Image
            </Button>
          </div>
        </div>
        <p className="text-[11px] text-muted-foreground">
          {activeId
            ? "Fine-tune the selected element's rotation and stacking."
            : 'Click an element on the canvas to fine-tune its rotation and stacking.'}
        </p>
      </div>

      {activeId ? (
        <ActiveElementPanel
          key={activeId}
          activeId={activeId}
          transform={controls.transform}
          textElement={controls.textElement}
          imageElement={controls.imageElement}
          locale={locale}
          canvas={controls.canvas}
          defaultTextColor={defaultTextColor}
          onRotate={controls.rotate}
          onReorder={controls.reorder}
          onTextChange={controls.setText}
          onTextPatch={controls.patchText}
          onDeleteText={controls.deleteText}
          onImageSource={controls.setImageSource}
          onImagePatch={controls.patchImage}
          onDeleteImage={controls.deleteImage}
        />
      ) : (
        <div className="rounded border border-dashed bg-background/40 p-4 text-center text-[11px] text-muted-foreground">
          No element selected
        </div>
      )}
    </div>
  )
}
