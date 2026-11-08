import { ArrowDownToLine, ArrowUpToLine, ChevronDown, ChevronUp, ImagePlus, RotateCw, Trash2, Type } from 'lucide-react'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { elementLabel, type ReorderDirection } from '@/lib/element-geometry'
import type { ElementId, ElementTransform, ImageElement, Patch, TextElement } from '@/lib/types'
import { ImageElementPanel } from './image-element-panel'
import { LayerButton } from './layer-button'
import { TextElementPanel } from './text-element-panel'

export function ActiveElementPanel({
  activeId,
  transform,
  textElement,
  imageElement,
  locale,
  canvas,
  defaultTextColor,
  onRotate,
  onReorder,
  onTextChange,
  onTextPatch,
  onDeleteText,
  onImagePatch,
  onImageSource,
  onDeleteImage,
}: {
  activeId: ElementId
  transform: ElementTransform | undefined
  textElement: TextElement | undefined
  imageElement: ImageElement | undefined
  locale: string
  canvas: { cW: number; cH: number }
  defaultTextColor: string
  onRotate: (rotation: number) => void
  onReorder: (dir: ReorderDirection) => void
  onTextChange: (value: string) => void
  onTextPatch: (patch: Patch<TextElement>) => void
  onDeleteText: () => void
  onImagePatch: (patch: Patch<ImageElement>) => void
  onImageSource: (src: string) => void
  onDeleteImage: () => void
}): ReactNode {
  const engaged = !!transform
  const rotation = transform?.rotation ?? 0
  const label = elementLabel(activeId)
  const isOverlay = !!textElement || !!imageElement
  const deleteLabel = textElement ? 'Delete text element' : 'Delete image element'
  return (
    <div className="space-y-2 rounded border bg-background/60 p-2.5">
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1 text-xs font-medium">
          {textElement && <Type className="h-3.5 w-3.5" />}
          {imageElement && <ImagePlus className="h-3.5 w-3.5" />}
          {label}
        </span>
        {isOverlay && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-6 w-6 hover:text-destructive"
            onClick={textElement ? onDeleteText : onDeleteImage}
            title={deleteLabel}
            aria-label={deleteLabel}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
        {!isOverlay && !engaged && <span className="text-[10px] text-muted-foreground">drag to enable</span>}
      </div>

      {textElement && (
        <TextElementPanel
          key={textElement.id}
          element={textElement}
          locale={locale}
          canvas={canvas}
          defaultColor={defaultTextColor}
          onTextChange={onTextChange}
          onTextPatch={onTextPatch}
        />
      )}

      {imageElement && (
        <ImageElementPanel element={imageElement} onPatch={onImagePatch} onSourceChange={onImageSource} />
      )}

      <div className="space-y-1">
        <div className="flex items-center justify-between">
          <Label className="flex items-center gap-1 text-[11px] text-muted-foreground">
            <RotateCw className="h-3 w-3" /> Rotation
          </Label>
          <span className="text-[11px] tabular-nums text-muted-foreground">{rotation}°</span>
        </div>
        <input
          type="range"
          min={-180}
          max={180}
          step={1}
          value={rotation}
          disabled={!engaged}
          onChange={(e) => onRotate(Number(e.target.value))}
          className="w-full disabled:opacity-50"
          aria-label={`${label} rotation`}
        />
      </div>

      <div className="space-y-1">
        <Label className="text-[11px] text-muted-foreground">Layer</Label>
        <div className="grid grid-cols-4 gap-1">
          <LayerButton disabled={!engaged} onClick={() => onReorder('back')} label="Send to back">
            <ArrowDownToLine className="h-3.5 w-3.5" />
          </LayerButton>
          <LayerButton disabled={!engaged} onClick={() => onReorder('down')} label="Send backward">
            <ChevronDown className="h-3.5 w-3.5" />
          </LayerButton>
          <LayerButton disabled={!engaged} onClick={() => onReorder('up')} label="Bring forward">
            <ChevronUp className="h-3.5 w-3.5" />
          </LayerButton>
          <LayerButton disabled={!engaged} onClick={() => onReorder('front')} label="Bring to front">
            <ArrowUpToLine className="h-3.5 w-3.5" />
          </LayerButton>
        </div>
      </div>
    </div>
  )
}
