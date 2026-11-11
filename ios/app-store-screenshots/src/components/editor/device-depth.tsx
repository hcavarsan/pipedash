'use client'
import type { ReactNode } from 'react'
import { alpha } from '@/lib/scene-colors'
import type { Scene } from '@/lib/types'

/**
 * Shadow, glow and 3D tilt around a device frame. The wrapper is always the
 * same shape, even in classic scenes, so toggling depth never remounts the
 * frame and re-decodes its (large) bezel image.
 */
export function DeviceDepth({
  scene,
  cW,
  accent,
  children,
}: {
  scene: Scene
  cW: number
  accent: string
  children: ReactNode
}): ReactNode {
  const s = scene.shadow / 100
  const g = scene.glow / 100
  return (
    <div style={{ position: 'relative', width: '100%', height: '100%' }}>
      {g > 0 && (
        <div
          aria-hidden
          style={{
            position: 'absolute',
            inset: '-14% -38%',
            background: `radial-gradient(closest-side, ${alpha(accent, 0.95 * g)} 0%, ${alpha(accent, 0.42 * g)} 55%, transparent 100%)`,
            pointerEvents: 'none',
          }}
        />
      )}
      <div
        style={{
          position: 'relative',
          width: '100%',
          height: '100%',
          filter:
            s > 0 ? `drop-shadow(0 ${cW * 0.03 * s}px ${cW * 0.045 * s}px rgba(0,0,0,${0.12 + 0.33 * s}))` : undefined,
        }}
      >
        <div
          style={{
            width: '100%',
            height: '100%',
            transform: scene.tilt
              ? `perspective(${cW * 2.6}px) rotateY(${scene.tilt}deg) rotateX(${Math.abs(scene.tilt) * 0.22}deg)`
              : undefined,
            transformOrigin: 'center center',
          }}
        >
          {children}
        </div>
      </div>
    </div>
  )
}
