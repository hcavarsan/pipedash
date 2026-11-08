'use client'
import { Maximize2, ZoomIn, ZoomOut } from 'lucide-react'
import { type ReactNode, useCallback } from 'react'
import { Button } from '@/components/ui/button'
import { getCanvas } from '@/lib/canvas-device'
import { DEVICE_LABEL } from '@/lib/device-specs'
import { layoutLabel } from '@/lib/layout-labels'
import type {
  Device,
  ElementId,
  ElementTransform,
  Orientation,
  Scene,
  SelectedElement,
  Slide,
  Theme,
} from '@/lib/types'
import type { DeckEditHandlers } from './canvas-types'
import { DeckCanvas } from './deck-canvas'
import { PlacementPreviews } from './placement-previews'
import { usePreviewViewport } from './use-preview-viewport'

type Props = {
  slides: Slide[]
  activeSlideId: string | null
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  appName?: string | undefined
  appIcon?: string | undefined
  fontFamily: string
  connectedCanvas: boolean
  scene?: Scene | undefined
  selectedElement: SelectedElement | null
  onActiveSlideChange: (id: string) => void
  onLabelChange: (slide: Slide, v: string) => void
  onHeadlineChange: (slide: Slide, v: string) => void
  onTextElementTextChange: (slideId: string, id: string, v: string) => void
  onElementChange: (slideId: string, id: ElementId, t: ElementTransform) => void
  onSelectElement: (element: SelectedElement | null) => void
}

type BadgeProps = {
  device: Device
  orientation: Orientation
  connectedCanvas: boolean
  activeSlide: Slide | null
  activeIndex: number
}

function StageBadge({ device, orientation, connectedCanvas, activeSlide, activeIndex }: BadgeProps): ReactNode {
  return (
    <div className="pointer-events-none absolute left-4 top-4 flex items-center gap-1.5 rounded-md bg-background/80 px-2 py-1 text-[11px] text-muted-foreground shadow-xs backdrop-blur-sm">
      <span className="font-medium text-foreground">{DEVICE_LABEL[device]}</span>
      {activeSlide && (
        <>
          <span aria-hidden>·</span>
          <span>Screen {activeIndex + 1}</span>
          <span aria-hidden>·</span>
          <span>{layoutLabel(activeSlide.layout, device)}</span>
        </>
      )}
      {orientation === 'landscape' && (
        <>
          <span aria-hidden>·</span>
          <span>landscape</span>
        </>
      )}
      {!connectedCanvas && (
        <>
          <span aria-hidden>·</span>
          <span>isolated</span>
        </>
      )}
    </div>
  )
}

type ZoomProps = {
  count: number
  cW: number
  cH: number
  scale: number
  zoom: number
  onStep: (delta: number) => void
  onFit: () => void
}

function ZoomControls({ count, cW, cH, scale, zoom, onStep, onFit }: ZoomProps): ReactNode {
  return (
    <div className="absolute bottom-4 right-4 flex items-center gap-1.5 rounded-md bg-background/85 px-1.5 py-1 text-[10px] tabular-nums text-muted-foreground shadow-xs backdrop-blur-sm">
      <span className="px-1">
        {count}× {cW}×{cH}
      </span>
      <span aria-hidden className="text-border">
        |
      </span>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="h-6 w-6"
        onClick={() => onStep(-0.1)}
        disabled={zoom <= 0.25}
        title="Zoom out"
        aria-label="Zoom out"
      >
        <ZoomOut className="h-3.5 w-3.5" />
      </Button>
      <span className="min-w-10 text-center">{(scale * 100).toFixed(0)}%</span>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="h-6 w-6"
        onClick={() => onStep(0.1)}
        disabled={zoom >= 2}
        title="Zoom in"
        aria-label="Zoom in"
      >
        <ZoomIn className="h-3.5 w-3.5" />
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="h-6 w-6"
        onClick={onFit}
        title="Fit active screen"
        aria-label="Fit active screen"
      >
        <Maximize2 className="h-3.5 w-3.5" />
      </Button>
    </div>
  )
}

// Fits one full-resolution screen inside the viewport while keeping the whole
// deck horizontally scrollable as one connected canvas.
export function PreviewStage({
  slides,
  activeSlideId,
  device,
  orientation,
  theme,
  locale,
  appName,
  appIcon,
  fontFamily,
  connectedCanvas,
  scene,
  selectedElement,
  onActiveSlideChange,
  onLabelChange,
  onHeadlineChange,
  onTextElementTextChange,
  onElementChange,
  onSelectElement,
}: Props): ReactNode {
  const { cW, cH } = getCanvas(device, orientation)
  const totalW = Math.max(1, slides.length) * cW
  const activeIndex = Math.max(
    0,
    slides.findIndex((slide) => slide.id === activeSlideId)
  )
  const activeSlide = slides[activeIndex] || slides[0] || null
  const { containerRef, scrollerRef, scale, zoom, stepZoom, fitActiveScreen, suppressNextPan } = usePreviewViewport({
    deckKey: `${device}:${orientation}`,
    cW,
    cH,
    activeIndex,
    activeId: activeSlide?.id,
  })

  const handleCanvasActiveSlideChange = useCallback(
    (id: string) => {
      if (id !== activeSlideId) {
        suppressNextPan()
      }
      onActiveSlideChange(id)
    },
    [activeSlideId, onActiveSlideChange, suppressNextPan]
  )

  const edit: DeckEditHandlers = {
    onLabelChange: (slideId, value) => {
      const slide = slides.find((s) => s.id === slideId)
      if (slide) {
        onLabelChange(slide, value)
      }
    },
    onHeadlineChange: (slideId, value) => {
      const slide = slides.find((s) => s.id === slideId)
      if (slide) {
        onHeadlineChange(slide, value)
      }
    },
    onTextElementTextChange,
    onElementChange,
    onSelectElement,
    onSelectScreen: handleCanvasActiveSlideChange,
  }

  return (
    <div
      ref={containerRef}
      className="relative h-full w-full overflow-hidden bg-[radial-gradient(70%_70%_at_50%_35%,hsl(var(--background))_0%,hsl(var(--muted))_100%)]"
    >
      <div ref={scrollerRef} className="h-full w-full overflow-auto p-12">
        <div
          style={{
            width: totalW * scale,
            height: cH * scale,
            position: 'relative',
            flexShrink: 0,
            filter: 'drop-shadow(0 32px 42px rgba(15, 23, 42, 0.18))',
          }}
        >
          <div
            style={{
              width: totalW,
              height: cH,
              transform: `scale(${scale})`,
              transformOrigin: 'top left',
            }}
          >
            <DeckCanvas
              slides={slides}
              device={device}
              orientation={orientation}
              theme={theme}
              locale={locale}
              appName={appName}
              appIcon={appIcon}
              fontFamily={fontFamily}
              connectedCanvas={connectedCanvas}
              scene={scene}
              editable
              previewScale={scale}
              selectedElement={selectedElement}
              activeSlideId={activeSlide?.id || null}
              showGuides
              edit={edit}
            />
          </div>
        </div>
      </div>

      <StageBadge
        device={device}
        orientation={orientation}
        connectedCanvas={connectedCanvas}
        activeSlide={activeSlide}
        activeIndex={activeIndex}
      />

      {device === 'creative-universal' && activeSlide && (
        <PlacementPreviews
          slide={activeSlide}
          index={activeIndex}
          count={slides.length}
          device={device}
          orientation={orientation}
          theme={theme}
          locale={locale}
          appName={appName}
          appIcon={appIcon}
          fontFamily={fontFamily}
          scene={scene}
        />
      )}

      <ZoomControls
        count={slides.length}
        cW={cW}
        cH={cH}
        scale={scale}
        zoom={zoom}
        onStep={stepZoom}
        onFit={fitActiveScreen}
      />
    </div>
  )
}
