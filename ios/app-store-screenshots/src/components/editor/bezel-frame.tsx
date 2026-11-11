'use client'
import type { ComponentType, CSSProperties, ReactNode } from 'react'
import { captureFits, DUO_FIT_TOLERANCE } from '@/lib/capture-fit'
import { bezelGeometry, type DuoDevice, type FrameGeometry, isDuoDevice } from '@/lib/frame-assets'
import { img } from '@/lib/image-cache'
import type { Device } from '@/lib/types'
import { ScreenImage } from './frame-empty-slot'
import type { FrameProps } from './frame-types'
import { PlainImg } from './plain-img'

// ---------- iPhone Duo ----------
// Geometry comes from Apple's bezel in public/frames/ (measured by
// /api/frames) or, without one, a drawn frame whose screen has the capture's
// exact aspect. The frame fits itself inside whatever box it is given, so a
// placement saved with a different bezel never stretches it.

function overlayStyle(aspect: number, rotate: boolean): CSSProperties {
  if (rotate) {
    return {
      position: 'absolute',
      left: '50%',
      top: '50%',
      width: `${100 / aspect}%`,
      height: `${100 * aspect}%`,
      transform: 'translate(-50%, -50%) rotate(-90deg)',
      pointerEvents: 'none',
    }
  }
  return { position: 'absolute', inset: 0, width: '100%', height: '100%', pointerEvents: 'none' }
}

function drawnBodyStyle(borderRadius: string): CSSProperties {
  return {
    position: 'absolute',
    inset: 0,
    borderRadius,
    background: 'linear-gradient(160deg, #3a3a3e 0%, #1b1b1e 45%, #2a2a2e 100%)',
    boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.14), inset 0 0 0 3px rgba(0,0,0,0.5), 0 10px 44px rgba(0,0,0,0.45)',
  }
}

function screenFit(device: Device, src: string, screenAspect: number): 'cover' | 'contain' {
  if (device === 'mac') {
    return 'cover'
  }
  return captureFits(src, screenAspect, isDuoDevice(device) ? DUO_FIT_TOLERANCE : undefined) ? 'cover' : 'contain'
}

// The outer display's front camera, where Apple's bezels put it: top right in
// portrait, top left once the closed phone is turned.
function FrontCamera({ screenAspect }: { screenAspect: number }): ReactNode {
  return (
    <div
      aria-hidden
      style={{
        position: 'absolute',
        top: '3.4cqmin',
        ...(screenAspect > 1 ? { left: '3.4cqmin' } : { right: '3.4cqmin' }),
        width: '5.6cqmin',
        height: '5.6cqmin',
        background: '#050506',
        borderRadius: 999,
        pointerEvents: 'none',
      }}
    />
  )
}

// A device drawn under Apple's bezel (or, for iPhone Duo without one, a drawn
// stand-in). Mac captures are 16:10 but MacBook screens are a little taller, so
// a Mac capture fills the screen from the top and loses a sliver at the bottom,
// like the built-in Mac window; everything else is letterboxed if it doesn't fit.
function BezelFrame({ device, src, alt = '', style, hideEmpty }: FrameProps & { device: Device }): ReactNode {
  const g: FrameGeometry | null = bezelGeometry(device)
  if (!g) {
    return null
  }
  const frameRadius = `${(g.radius.x * g.screen.W) / 100 + g.screen.L}% / ${(g.radius.y * g.screen.H) / 100 + g.screen.T}%`
  const fit = screenFit(device, src, g.screenAspect)
  return (
    <div
      style={{
        position: 'relative',
        aspectRatio: `${g.aspect}`,
        ...style,
        containerType: 'size',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <div
        style={{ position: 'relative', width: `min(100cqw, calc(100cqh * ${g.aspect}))`, aspectRatio: `${g.aspect}` }}
      >
        {!g.image && <div style={drawnBodyStyle(frameRadius)} />}
        <div
          style={{
            position: 'absolute',
            left: `${g.screen.L}%`,
            top: `${g.screen.T}%`,
            width: `${g.screen.W}%`,
            height: `${g.screen.H}%`,
            borderRadius: g.clip ?? `${g.radius.x}% / ${g.radius.y}%`,
            overflow: 'hidden',
            background: '#000',
            containerType: 'size',
          }}
        >
          <ScreenImage
            src={src}
            alt={alt}
            hideEmpty={hideEmpty}
            objectFit={fit}
            objectPosition={device === 'mac' ? 'top' : 'center'}
          />
          {!g.image && g.camera === 'corner' && <FrontCamera screenAspect={g.screenAspect} />}
        </div>
        {g.image && (
          // The real bezel sits over the screen; its transparent cutout shows it.
          <PlainImg
            src={img(g.image.src)}
            alt=""
            data-export-check="full"
            draggable={false}
            style={overlayStyle(g.aspect, g.image.rotate)}
          />
        )}
      </div>
    </div>
  )
}

// One stable component per target, so switching decks doesn't remount frames.
export const DUO_FRAMES: Record<DuoDevice, ComponentType<FrameProps>> = {
  'duo-outer': (props) => <BezelFrame device="duo-outer" {...props} />,
  'duo-outer-landscape': (props) => <BezelFrame device="duo-outer-landscape" {...props} />,
  'duo-inner': (props) => <BezelFrame device="duo-inner" {...props} />,
  'duo-inner-landscape': (props) => <BezelFrame device="duo-inner-landscape" {...props} />,
}

const BEZEL_FRAMES: Partial<Record<Device, ComponentType<FrameProps>>> = {
  ...DUO_FRAMES,
  iphone: (props) => <BezelFrame device="iphone" {...props} />,
  ipad: (props) => <BezelFrame device="ipad" {...props} />,
  watchos: (props) => <BezelFrame device="watchos" {...props} />,
  tvos: (props) => <BezelFrame device="tvos" {...props} />,
  mac: (props) => <BezelFrame device="mac" {...props} />,
  'creative-universal': (props) => <BezelFrame device="creative-universal" {...props} />,
  'creative-header': (props) => <BezelFrame device="creative-header" {...props} />,
  'creative-search': (props) => <BezelFrame device="creative-search" {...props} />,
}

function NoBezel(): ReactNode {
  return null
}

/** The stable bezel frame component for a device (used once its bezel is measured). */
export function bezelFrameFor(device: Device): ComponentType<FrameProps> {
  return BEZEL_FRAMES[device] ?? NoBezel
}
