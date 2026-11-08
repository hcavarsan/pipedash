'use client'
import type { ReactNode } from 'react'
import { ScreenImage } from './frame-empty-slot'
import type { FrameProps } from './frame-types'

export function AndroidTabletP({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '5 / 8', ...style }}>
      <div
        style={{
          width: '100%',
          height: '100%',
          borderRadius: '4.5% / 2.8%',
          background: 'linear-gradient(160deg, #2a2a2e 0%, #18181b 100%)',
          boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.08), 0 8px 48px rgba(0,0,0,0.6)',
          position: 'relative',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            position: 'absolute',
            top: '1.2%',
            left: '50%',
            transform: 'translateX(-50%)',
            width: '1.4%',
            height: '0.88%',
            borderRadius: '50%',
            background: '#0d0d0f',
            zIndex: 20,
          }}
        />
        <div
          style={{
            position: 'absolute',
            left: '3.5%',
            top: '2.2%',
            width: '93%',
            height: '95.6%',
            borderRadius: '2.5% / 1.6%',
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

export function AndroidTabletL({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '8 / 5', ...style }}>
      <div
        style={{
          width: '100%',
          height: '100%',
          borderRadius: '2.8% / 4.5%',
          background: 'linear-gradient(160deg, #2a2a2e 0%, #18181b 100%)',
          boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.08), 0 8px 48px rgba(0,0,0,0.6)',
          position: 'relative',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            position: 'absolute',
            left: '1.2%',
            top: '50%',
            transform: 'translateY(-50%)',
            width: '0.88%',
            height: '1.4%',
            borderRadius: '50%',
            background: '#0d0d0f',
            zIndex: 20,
          }}
        />
        <div
          style={{
            position: 'absolute',
            left: '2.2%',
            top: '3.5%',
            width: '95.6%',
            height: '93%',
            borderRadius: '1.6% / 2.5%',
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

export function IPad({ src, alt = '', style, hideEmpty }: FrameProps): ReactNode {
  return (
    <div style={{ position: 'relative', aspectRatio: '770 / 1000', ...style }}>
      <div
        style={{
          width: '100%',
          height: '100%',
          borderRadius: '5% / 3.6%',
          background: 'linear-gradient(180deg, #2C2C2E 0%, #1C1C1E 100%)',
          position: 'relative',
          overflow: 'hidden',
          boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.1), 0 8px 40px rgba(0,0,0,0.6)',
        }}
      >
        <div
          style={{
            position: 'absolute',
            top: '1.2%',
            left: '50%',
            transform: 'translateX(-50%)',
            width: '0.9%',
            height: '0.65%',
            borderRadius: '50%',
            background: '#111113',
            zIndex: 20,
          }}
        />
        <div
          style={{
            position: 'absolute',
            left: '4%',
            top: '2.8%',
            width: '92%',
            height: '94.4%',
            borderRadius: '2.2% / 1.6%',
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
