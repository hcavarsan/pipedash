'use client'
import type { ReactNode } from 'react'
import { getCanvas } from '@/lib/canvas-device'
import { isFeatureGraphic } from '@/lib/canvas-geometry'
import { isCreative } from '@/lib/device-specs'
import { sceneOf } from '@/lib/scene'
import type { Device, Orientation, Scene, Slide, Theme } from '@/lib/types'
import { SlideBackground } from './canvas-background'
import { CreativeGuide, ScreenGuide } from './canvas-guides'
import type { DeckCanvasProps, DeckEditHandlers, EditHandlers, ElementLayer } from './canvas-types'
import { FeatureGraphicCanvas } from './feature-graphic-canvas'
import { SlideElements } from './slide-elements'

type DeckShared = {
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  scene: Scene
  editable?: boolean | undefined
  edit?: DeckEditHandlers | undefined
  cW: number
  cH: number
}

function slideEditHandlers(slideId: string, edit: DeckEditHandlers | undefined): EditHandlers {
  return {
    onLabelChange: (v) => edit?.onLabelChange?.(slideId, v),
    onHeadlineChange: (v) => edit?.onHeadlineChange?.(slideId, v),
    onTextElementTextChange: (id, v) => edit?.onTextElementTextChange?.(slideId, id, v),
    onElementChange: (id, t) => edit?.onElementChange?.(slideId, id, t),
    onSelectElement: (id) => {
      edit?.onSelectScreen?.(slideId)
      edit?.onSelectElement?.(id ? { slideId, elementId: id } : null)
    },
  }
}

function DeckScreen({
  slide,
  index,
  shared,
  appName,
  appIcon,
  active,
  showGuides,
  stripIndex,
  stripCount,
}: {
  slide: Slide
  index: number
  shared: DeckShared
  appName?: string | undefined
  appIcon?: string | undefined
  active: boolean
  showGuides: boolean
  stripIndex: number
  stripCount: number
}): ReactNode {
  const { device, theme, locale, scene, editable, edit, cW, cH } = shared
  const featureEdit: EditHandlers = {
    onHeadlineChange: (v) => edit?.onHeadlineChange?.(slide.id, v),
    onSelectElement: () => edit?.onSelectScreen?.(slide.id),
  }
  return (
    <div
      role="none"
      onMouseDown={(e) => {
        if (!editable || e.defaultPrevented) {
          return
        }
        edit?.onSelectScreen?.(slide.id)
        edit?.onSelectElement?.(null)
      }}
      style={{
        position: 'absolute',
        left: index * cW,
        top: 0,
        width: cW,
        height: cH,
        overflow: 'hidden',
      }}
    >
      {isFeatureGraphic(slide, device) ? (
        <FeatureGraphicCanvas
          slide={slide}
          cW={cW}
          theme={theme}
          locale={locale}
          appName={appName}
          appIcon={appIcon}
          editable={editable}
          edit={featureEdit}
        />
      ) : (
        <SlideBackground
          slide={slide}
          cW={cW}
          cH={cH}
          theme={theme}
          scene={scene}
          index={stripIndex}
          count={stripCount}
        />
      )}
      {showGuides && <ScreenGuide cW={cW} cH={cH} index={index} active={active} />}
    </div>
  )
}

function DeckSlideElements({
  slide,
  index,
  shared,
  connectedCanvas,
  totalW,
  selectedElementId,
  previewScale,
  hideEmpty,
}: {
  slide: Slide
  index: number
  shared: DeckShared
  connectedCanvas: boolean
  totalW: number
  selectedElementId: ElementLayer['selectedElementId']
  previewScale: number
  hideEmpty?: boolean | undefined
}): ReactNode {
  const { device, orientation, theme, locale, scene, editable, edit, cW, cH } = shared
  if (isFeatureGraphic(slide, device)) {
    return null
  }
  const layer: ElementLayer = {
    editable,
    edit: editable ? slideEditHandlers(slide.id, edit) : undefined,
    selectedElementId,
    previewScale,
    screenX: connectedCanvas ? index * cW : 0,
    boundsW: connectedCanvas ? totalW : cW,
    boundsH: cH,
    allowCrossScreen: connectedCanvas,
  }
  const elements = (
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
  )
  if (connectedCanvas) {
    return elements
  }
  return (
    <div
      style={{
        position: 'absolute',
        left: index * cW,
        top: 0,
        width: cW,
        height: cH,
        overflow: 'hidden',
      }}
    >
      {elements}
    </div>
  )
}

export function DeckCanvas({
  slides,
  device,
  orientation,
  theme,
  locale,
  appName,
  appIcon,
  fontFamily,
  connectedCanvas = true,
  editable,
  edit,
  selectedElement = null,
  activeSlideId = null,
  previewScale = 1,
  hideEmpty,
  showGuides = false,
  scene: rawScene,
  stripStart = 0,
  stripCount,
}: DeckCanvasProps): ReactNode {
  const { cW, cH } = getCanvas(device, orientation)
  const shared: DeckShared = { device, orientation, theme, locale, scene: sceneOf(rawScene), editable, edit, cW, cH }
  const totalW = Math.max(1, slides.length) * cW

  return (
    <div
      style={{
        width: totalW,
        height: cH,
        position: 'relative',
        overflow: 'hidden',
        fontFamily,
      }}
    >
      {slides.map((slide, index) => (
        <DeckScreen
          key={`${slide.id}-screen`}
          slide={slide}
          index={index}
          shared={shared}
          appName={appName}
          appIcon={appIcon}
          active={activeSlideId === slide.id}
          showGuides={showGuides}
          stripIndex={stripStart + index}
          stripCount={stripCount ?? slides.length}
        />
      ))}

      {slides.map((slide, index) => (
        <DeckSlideElements
          key={`${slide.id}-elements`}
          slide={slide}
          index={index}
          shared={shared}
          connectedCanvas={connectedCanvas}
          totalW={totalW}
          selectedElementId={selectedElement?.slideId === slide.id ? selectedElement.elementId : null}
          previewScale={previewScale}
          hideEmpty={hideEmpty}
        />
      ))}

      {showGuides &&
        isCreative(device) &&
        slides.map((slide, index) => (
          <CreativeGuide key={`${slide.id}-creative-guide`} device={device} cW={cW} cH={cH} left={index * cW} />
        ))}
    </div>
  )
}
