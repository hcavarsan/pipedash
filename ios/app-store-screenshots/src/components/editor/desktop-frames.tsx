'use client'
import type { ReactNode } from 'react'
import { MAC_RATIO, MAC_TITLE_BAR } from '@/lib/device-specs'
import { ScreenImage } from './frame-empty-slot'
import type { FrameProps } from './frame-types'

function TrafficLight({ color }: { color: string }): ReactNode {
  return (
    <span
      style={{
        width: '1.1%',
        aspectRatio: '1',
        borderRadius: '50%',
        background: color,
        boxShadow: 'inset 0 0 0 0.5px rgba(0,0,0,0.25)',
        flexShrink: 0,
      }}
    />
  )
}

// Mac window: title bar + a content area that is exactly 16:10, the Mac App
// Store's own aspect, so a full-screen or 16:10 capture fills it uncropped.
export function MacWindow({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  const titleBar = `${(MAC_TITLE_BAR / (1 + MAC_TITLE_BAR)) * 100}%`
  return (
    <div style={{ position: 'relative', aspectRatio: `${MAC_RATIO}`, ...style }}>
      <div
        style={{
          width: '100%',
          height: '100%',
          // Circular corners: the vertical radius is scaled by the frame aspect.
          borderRadius: `1.1% / ${1.1 * MAC_RATIO}%`,
          background: '#1C1C1E',
          boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.12), 0 18px 60px rgba(0,0,0,0.40)',
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <div
          style={{
            height: titleBar,
            boxSizing: 'border-box',
            display: 'flex',
            alignItems: 'center',
            gap: '0.75%',
            padding: '0 1.3%',
            background: 'linear-gradient(180deg, #3D3D40 0%, #323235 100%)',
            boxShadow: 'inset 0 -1px 0 rgba(0,0,0,0.35)',
            flexShrink: 0,
          }}
        >
          <TrafficLight color="#FF5F57" />
          <TrafficLight color="#FEBC2E" />
          <TrafficLight color="#28C840" />
        </div>
        <div style={{ flex: 1, overflow: 'hidden', background: '#111', minHeight: 0 }}>
          <ScreenImage src={src} alt={alt} hideEmpty={hideEmpty} />
        </div>
      </div>
    </div>
  )
}

// ---------- Apple TV ----------
// The screenshot IS the content, so there is no device UI to draw. A thin dark
// bezel reads as "TV" without competing with the capture. `bare` renders the
// capture edge-to-edge with no bezel at all.
export function AppleTV({
  src,
  alt = '',
  style,
  hideEmpty,
  bare,
}: FrameProps & { bare?: boolean | undefined }): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '16 / 9', ...style }}>
      <div
        style={{
          width: '100%',
          height: '100%',
          borderRadius: bare ? '0.6% / 1.1%' : '1.1% / 2.0%',
          background: bare ? 'transparent' : '#0A0A0C',
          padding: bare ? '0' : '0.9%',
          boxSizing: 'border-box',
          boxShadow: bare
            ? '0 18px 60px rgba(0,0,0,0.28)'
            : '0 22px 70px rgba(0,0,0,0.40), inset 0 0 0 1px rgba(255,255,255,0.06)',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            width: '100%',
            height: '100%',
            overflow: 'hidden',
            background: '#111',
            borderRadius: bare ? '0.6% / 1.1%' : '0.5% / 0.9%',
          }}
        >
          <ScreenImage src={src} alt={alt} hideEmpty={hideEmpty} objectPosition="center" />
        </div>
      </div>
    </div>
  )
}

// ---------- Apple Watch ----------
// Cushion-shaped body with a very large corner radius, plus the digital crown and
// side button on the right. The bezel is proportionally much thicker than a phone's,
// which is what makes a watch read as a watch at small sizes.
export function AppleWatch({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '422 / 514', ...style }}>
      {/* crown + side button */}
      <div
        style={{
          position: 'absolute',
          right: '-2.6%',
          top: '27%',
          width: '3.4%',
          height: '12%',
          background: 'linear-gradient(180deg,#8E8E93,#5A5A5E)',
          borderRadius: '40%',
        }}
      />
      <div
        style={{
          position: 'absolute',
          right: '-1.8%',
          top: '45%',
          width: '2.4%',
          height: '14%',
          background: 'linear-gradient(180deg,#6E6E73,#48484A)',
          borderRadius: '40%',
        }}
      />
      <div
        style={{
          width: '100%',
          height: '100%',
          borderRadius: '26% / 21%',
          background: '#0A0A0C',
          padding: '6.5%',
          boxSizing: 'border-box',
          boxShadow: '0 18px 50px rgba(0,0,0,0.42), inset 0 0 0 1px rgba(255,255,255,0.08)',
          overflow: 'hidden',
        }}
      >
        <div
          style={{ width: '100%', height: '100%', overflow: 'hidden', background: '#000', borderRadius: '22% / 18%' }}
        >
          <ScreenImage src={src} alt={alt} hideEmpty={hideEmpty} objectPosition="center" />
        </div>
      </div>
    </div>
  )
}

// ---------- CarPlay ----------
// A dashboard head unit: wide, squared-off, minimal bezel. Deliberately plainer than
// the TV frame because CarPlay screens are set into a fascia rather than being a
// product silhouette anyone recognises.
export function CarPlayScreen({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '800 / 480', ...style }}>
      <div
        style={{
          width: '100%',
          height: '100%',
          borderRadius: '2.2% / 3.6%',
          background: '#0B0B0D',
          padding: '1.6%',
          boxSizing: 'border-box',
          boxShadow: '0 20px 60px rgba(0,0,0,0.40), inset 0 0 0 1px rgba(255,255,255,0.07)',
          overflow: 'hidden',
        }}
      >
        <div
          style={{ width: '100%', height: '100%', overflow: 'hidden', background: '#000', borderRadius: '1.4% / 2.3%' }}
        >
          <ScreenImage src={src} alt={alt} hideEmpty={hideEmpty} objectPosition="center" />
        </div>
      </div>
    </div>
  )
}
