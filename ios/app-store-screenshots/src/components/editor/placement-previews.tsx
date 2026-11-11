import type { ReactNode } from 'react'
import { getCanvas } from '@/lib/canvas-device'
import { CREATIVE_SPECS, centredCrop } from '@/lib/device-specs'
import type { Device, Orientation, Scene, Slide, Theme } from '@/lib/types'
import { SlideCanvas } from './slide-canvas'

type CropProps = {
  label: string
  aspect: number
  cW: number
  cH: number
  children: ReactNode
}

function PlacementCrop({ label, aspect, cW, cH, children }: CropProps): ReactNode {
  const r = centredCrop(cW, cH, aspect)
  const width = aspect > 2 ? 240 : 150
  const k = width / r.w
  return (
    <figure className="space-y-1">
      <div className="relative overflow-hidden rounded-sm ring-1 ring-black/10" style={{ width, height: r.h * k }}>
        <div
          style={{
            position: 'absolute',
            left: 0,
            top: 0,
            width: cW,
            height: cH,
            transform: `scale(${k}) translate(${-r.x}px, ${-r.y}px)`,
            transformOrigin: 'top left',
          }}
        >
          {children}
        </div>
      </div>
      <figcaption className="text-[10px] text-muted-foreground">
        {label.replace(' crop', '')} (estimated crop)
      </figcaption>
    </figure>
  )
}

type Props = {
  slide: Slide
  index: number
  count: number
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  appName: string | undefined
  appIcon: string | undefined
  fontFamily: string
  scene: Scene | undefined
}

// How the active universal creative reads in each App Store placement, using
// the same estimated crops as the canvas guides. Rendered like an export.
export function PlacementPreviews({
  slide,
  index,
  count,
  device,
  orientation,
  theme,
  locale,
  appName,
  appIcon,
  fontFamily,
  scene,
}: Props): ReactNode {
  const { cW, cH } = getCanvas(device, orientation)
  const crops = device === 'creative-universal' ? CREATIVE_SPECS[device].crops : []
  const canvas = (
    <SlideCanvas
      slide={slide}
      device={device}
      orientation={orientation}
      theme={theme}
      locale={locale}
      appName={appName}
      appIcon={appIcon}
      fontFamily={fontFamily}
      scene={scene}
      stripIndex={index}
      stripCount={count}
      hideEmpty
    />
  )
  return (
    <fieldset
      aria-label="Placement previews"
      className="pointer-events-none absolute bottom-4 left-4 hidden items-end gap-3 rounded-md bg-background/85 p-2 shadow-xs backdrop-blur-sm md:flex"
    >
      {crops.map((crop) => (
        <PlacementCrop key={crop.label} label={crop.label} aspect={crop.aspect} cW={cW} cH={cH}>
          {canvas}
        </PlacementCrop>
      ))}
    </fieldset>
  )
}
