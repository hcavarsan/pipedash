'use client'
import type { MouseEvent, ReactNode } from 'react'
import { getCanvas } from '@/lib/canvas-device'
import { isFeatureGraphic } from '@/lib/canvas-geometry'
import { sceneOf } from '@/lib/scene'
import { SlideBackground } from './canvas-background'
import type { ElementLayer, SlideCanvasProps } from './canvas-types'
import { FeatureGraphicCanvas } from './feature-graphic-canvas'
import { SlideElements } from './slide-elements'

// ---------- Main single-screen canvas ----------

export function SlideCanvas({
  slide,
  device,
  orientation,
  theme,
  locale,
  appName,
  appIcon,
  fontFamily,
  editable,
  edit,
  selectedElementId = null,
  previewScale = 1,
  hideEmpty,
  scene: rawScene,
  stripIndex = 0,
  stripCount = 1,
}: SlideCanvasProps): ReactNode {
  const { cW, cH } = getCanvas(device, orientation)
  const scene = sceneOf(rawScene)

  if (isFeatureGraphic(slide, device)) {
    return (
      <div style={{ width: '100%', height: '100%', fontFamily }}>
        <FeatureGraphicCanvas
          slide={slide}
          cW={cW}
          theme={theme}
          locale={locale}
          appName={appName}
          appIcon={appIcon}
          editable={editable}
          edit={edit}
        />
      </div>
    )
  }

  const layer: ElementLayer = {
    editable,
    edit,
    selectedElementId,
    previewScale,
    screenX: 0,
    boundsW: cW,
    boundsH: cH,
    allowCrossScreen: false,
  }
  const handleBackgroundMouseDown = (e: MouseEvent<HTMLDivElement>) => {
    if (editable && e.target === e.currentTarget) {
      edit?.onSelectElement?.(null)
    }
  }

  return (
    <div
      role="none"
      onMouseDown={handleBackgroundMouseDown}
      style={{
        width: '100%',
        height: '100%',
        position: 'relative',
        overflow: 'hidden',
        fontFamily,
      }}
    >
      <SlideBackground
        slide={slide}
        cW={cW}
        cH={cH}
        theme={theme}
        scene={scene}
        index={stripIndex}
        count={stripCount}
      />
      <SlideElements
        slide={slide}
        scene={scene}
        device={device}
        orientation={orientation}
        theme={theme}
        locale={locale}
        hideEmpty={hideEmpty}
        layer={layer}
      />
    </div>
  )
}
