'use client'
import type { ReactNode } from 'react'
import { toImageElementId } from '@/lib/elements'
import type { ImageElement } from '@/lib/types'
import type { ElementLayer } from './canvas-types'
import { ImageElementCanvas } from './image-element-canvas'
import { LayerMovable } from './layer-movable'

export function CanvasImageElement({
  layer,
  element,
  index,
}: {
  layer: ElementLayer
  element: ImageElement
  index: number
}): ReactNode {
  const rect = element.transform
  return (
    <LayerMovable
      layer={layer}
      elementId={toImageElementId(element.id)}
      rect={rect}
      rotation={rect.rotation ?? 0}
      zIndex={rect.zIndex ?? 5 + index}
      allowOverflow={layer.allowCrossScreen}
    >
      <ImageElementCanvas element={element} editable={layer.editable} />
    </LayerMovable>
  )
}
