'use client'
import type { ReactNode } from 'react'
import type { Scene, Slide, Theme } from '@/lib/types'
import { Caption } from './canvas-caption'
import type { CaptionRect, ElementLayer } from './canvas-types'
import { LayerMovable } from './layer-movable'

export function CanvasCaptionElement({
  layer,
  slide,
  scene,
  theme,
  locale,
  rect,
  cW,
  cH,
  typeUnit,
}: {
  layer: ElementLayer
  slide: Slide
  scene: Scene
  theme: Theme
  locale: string
  rect: CaptionRect
  cW: number
  cH: number
  typeUnit: number | undefined
}): ReactNode {
  const saved = slide.transforms?.caption
  const align = scene.captionAlign === 'auto' ? rect.align || 'center' : scene.captionAlign
  return (
    <LayerMovable
      layer={layer}
      elementId="caption"
      rect={rect}
      rotation={saved?.rotation ?? 0}
      zIndex={saved?.zIndex ?? 4}
      allowOverflow={layer.allowCrossScreen}
    >
      <div style={{ width: '100%', height: '100%', display: 'flex', alignItems: 'flex-start' }}>
        <Caption
          cW={cW}
          cH={cH}
          slide={slide}
          theme={theme}
          locale={locale}
          editable={layer.editable}
          edit={layer.edit}
          align={align}
          inverted={!!slide.inverted}
          scene={scene}
          typeUnit={typeUnit}
          onFocus={() => layer.edit?.onSelectElement?.('caption')}
        />
      </div>
    </LayerMovable>
  )
}
