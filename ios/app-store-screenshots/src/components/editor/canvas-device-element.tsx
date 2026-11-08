'use client'
import type { CSSProperties, ReactNode } from 'react'
import { DUO_COMPANION, type DuoDevice, duoGeometry } from '@/lib/frame-assets'
import type { Scene, Slide, Theme } from '@/lib/types'
import { DUO_FRAMES } from './bezel-frame'
import type { ElementLayer, FrameComp, Rect } from './canvas-types'
import { DeviceDepth } from './device-depth'
import { LayerMovable } from './layer-movable'

export function CanvasDeviceElement({
  layer,
  slide,
  scene,
  theme,
  slot,
  rect,
  src,
  extraStyle,
  Frame,
  frameAspect,
  duoDevice,
  cW,
  hideEmpty,
}: {
  layer: ElementLayer
  slide: Slide
  scene: Scene
  theme: Theme
  slot: 'device' | 'deviceSecondary'
  rect: Rect
  src: string
  extraStyle?: CSSProperties | undefined
  Frame: FrameComp
  frameAspect: number
  // On iPhone Duo the back device is the same phone in its other state. Duo
  // frames also ignore the scene's tilt: Apple's marketing guidelines ask for
  // straight-on product images.
  duoDevice: DuoDevice | null
  cW: number
  hideEmpty: boolean | undefined
}): ReactNode {
  const midnight = theme.id === 'midnight-glow-pro'
  const companion = duoDevice ? DUO_COMPANION[duoDevice] : null
  const secondaryCompanion = slot === 'deviceSecondary' ? companion : null
  const DeviceFrame = secondaryCompanion ? DUO_FRAMES[secondaryCompanion] : Frame
  const aspect = secondaryCompanion ? duoGeometry(secondaryCompanion).aspect : frameAspect
  const saved = slide.transforms?.[slot]
  const frame = <DeviceFrame src={src} hideEmpty={hideEmpty} style={{ width: '100%', height: '100%', ...extraStyle }} />
  return (
    <LayerMovable
      layer={layer}
      elementId={slot}
      rect={rect}
      rotation={midnight ? 0 : (saved?.rotation ?? 0)}
      zIndex={saved?.zIndex ?? (slot === 'deviceSecondary' ? 2 : 3)}
      lockAspectRatio={aspect}
      allowOverflow
    >
      {midnight ? (
        frame
      ) : (
        <DeviceDepth scene={companion ? { ...scene, tilt: 0 } : scene} cW={cW} accent={theme.accent}>
          {frame}
        </DeviceDepth>
      )}
    </LayerMovable>
  )
}
