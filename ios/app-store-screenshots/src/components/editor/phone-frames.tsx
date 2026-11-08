'use client'
import type { ReactNode } from 'react'
import { PHONE_SCREEN } from '@/lib/device-specs'
import { img } from '@/lib/image-cache'
import { ScreenImage } from './frame-empty-slot'
import type { FrameProps } from './frame-types'
import { PlainImg } from './plain-img'

// iPhone — uses pre-measured mockup.png overlay
export function Phone({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '1022 / 2082', ...style }}>
      {/* The screen layer covers the bezel's centre, so the exporter checks the
          whole mockup (its visible bezel) rather than its hidden middle. */}
      <PlainImg
        src={img('/mockup.png')}
        alt=""
        data-export-check="full"
        style={{ display: 'block', width: '100%', height: '100%' }}
        draggable={false}
      />
      <div
        style={{
          position: 'absolute',
          zIndex: 10,
          overflow: 'hidden',
          left: `${PHONE_SCREEN.L}%`,
          top: `${PHONE_SCREEN.T}%`,
          width: `${PHONE_SCREEN.W}%`,
          height: `${PHONE_SCREEN.H}%`,
          borderRadius: `${PHONE_SCREEN.RX}% / ${PHONE_SCREEN.RY}%`,
          background: '#111',
        }}
      >
        <ScreenImage src={src} alt={alt} hideEmpty={hideEmpty} />
      </div>
    </div>
  )
}

export function AndroidPhone({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '9 / 19.5', ...style }}>
      <div
        style={{
          width: '100%',
          height: '100%',
          borderRadius: '8% / 4%',
          background: 'linear-gradient(160deg, #2a2a2e 0%, #18181b 100%)',
          boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.08), 0 8px 40px rgba(0,0,0,0.55)',
          position: 'relative',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            position: 'absolute',
            top: '1.5%',
            left: '50%',
            transform: 'translateX(-50%)',
            width: '3%',
            height: '1.4%',
            borderRadius: '50%',
            background: '#0d0d0f',
            border: '1px solid rgba(255,255,255,0.06)',
            zIndex: 20,
          }}
        />
        <div
          style={{
            position: 'absolute',
            left: '3.5%',
            top: '2%',
            width: '93%',
            height: '96%',
            borderRadius: '5.5% / 2.6%',
            overflow: 'hidden',
            background: '#000',
          }}
        >
          <ScreenImage src={src} alt={alt} hideEmpty={hideEmpty} />
        </div>
      </div>
    </div>
  )
}
