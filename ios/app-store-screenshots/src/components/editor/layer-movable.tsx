'use client'
import type { ReactNode } from 'react'
import type { ElementId } from '@/lib/types'
import type { ElementLayer, Rect } from './canvas-types'
import { Movable } from './movable'

/** A Movable placed on a screen that may sit anywhere in a connected strip. */
export function LayerMovable({
  layer,
  elementId,
  rect,
  rotation,
  zIndex,
  allowOverflow,
  lockAspectRatio,
  children,
}: {
  layer: ElementLayer
  elementId: ElementId
  rect: Rect
  rotation: number
  zIndex: number
  allowOverflow: boolean
  lockAspectRatio?: number | undefined
  children: ReactNode
}): ReactNode {
  const { edit, screenX } = layer
  return (
    <Movable
      rect={{ x: rect.x + screenX, y: rect.y, width: rect.width, height: rect.height }}
      boundsW={layer.boundsW}
      boundsH={layer.boundsH}
      editable={layer.editable}
      previewScale={layer.previewScale}
      rotation={rotation}
      onChange={(t) =>
        edit?.onElementChange?.(elementId, {
          ...t,
          x: t.x - screenX,
          rotation: t.rotation ?? rotation,
          zIndex: t.zIndex ?? zIndex,
        })
      }
      lockAspectRatio={lockAspectRatio}
      zIndex={zIndex}
      allowOverflow={allowOverflow}
      selected={layer.selectedElementId === elementId}
      onSelect={() => edit?.onSelectElement?.(elementId)}
    >
      {children}
    </Movable>
  )
}
