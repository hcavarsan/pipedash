'use client'
import type { ReactNode } from 'react'
import { defaultElementZ, SCREEN_WIDTH_FRACTION } from '@/lib/canvas-geometry'
import type { Callout, Slide, Theme } from '@/lib/types'
import { CalloutLoupe } from './callout-loupe'
import type { ElementLayer, Rect } from './canvas-types'
import { LayerMovable } from './layer-movable'

export function CanvasCalloutElement({
  layer,
  slide,
  theme,
  callout,
  rect,
  deviceWidth,
  src,
  cW,
  hideEmpty,
}: {
  layer: ElementLayer
  slide: Slide
  theme: Theme
  callout: Callout
  rect: Rect
  deviceWidth: number
  src: string
  cW: number
  hideEmpty: boolean | undefined
}): ReactNode {
  const saved = slide.transforms?.callout
  return (
    <LayerMovable
      layer={layer}
      elementId="callout"
      rect={rect}
      rotation={saved?.rotation ?? 0}
      zIndex={saved?.zIndex ?? defaultElementZ('callout')}
      lockAspectRatio={callout.shape === 'circle' ? 1 : undefined}
      allowOverflow
    >
      <CalloutLoupe
        callout={callout}
        src={src}
        screenWidth={deviceWidth * SCREEN_WIDTH_FRACTION}
        shortSide={Math.min(rect.width, rect.height)}
        cW={cW}
        accent={theme.accent}
        hideEmpty={hideEmpty}
      />
    </LayerMovable>
  )
}
