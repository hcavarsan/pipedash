'use client'
import type { ReactNode } from 'react'
import { img } from '@/lib/image-cache'
import { alpha } from '@/lib/scene-colors'
import type { Callout } from '@/lib/types'
import { PlainImg } from './plain-img'

/**
 * A loupe showing part of the screenshot at `zoom`× the size it appears on the
 * device. `screenWidth` is the device's on-canvas screen width in canvas px;
 * `shortSide` is the lens's shorter side in canvas px.
 */
export function CalloutLoupe({
  callout,
  src,
  screenWidth,
  shortSide,
  cW,
  accent,
  hideEmpty,
}: {
  callout: Callout
  src: string
  screenWidth: number
  shortSide: number
  cW: number
  accent: string
  hideEmpty?: boolean | undefined
}): ReactNode {
  const resolved = img(src)
  const border = Math.max(3, cW * 0.009)
  // A % radius resolves against width and height separately, so a wide lens
  // got stretched elliptical corners. A px radius from the shorter side stays
  // round and matches the old 14% on a square lens.
  const radius = callout.shape === 'circle' ? '50%' : `${shortSide * 0.14}px`
  // An export without a screenshot shouldn't ship an empty lens.
  if (!resolved && hideEmpty) {
    return null
  }
  return (
    <div
      style={{
        position: 'relative',
        width: '100%',
        height: '100%',
        borderRadius: radius,
        overflow: 'hidden',
        background: '#0f0f12',
        boxShadow: `0 0 0 ${border}px #ffffff, 0 ${cW * 0.025}px ${cW * 0.06}px rgba(0,0,0,0.32)`,
      }}
    >
      {resolved ? (
        <PlainImg
          src={resolved}
          alt=""
          draggable={false}
          style={{
            position: 'absolute',
            left: '50%',
            top: '50%',
            width: screenWidth * callout.zoom,
            maxWidth: 'none',
            height: 'auto',
            transform: `translate(${-callout.focusX * 100}%, ${-callout.focusY * 100}%)`,
          }}
        />
      ) : (
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: 'rgba(255,255,255,0.7)',
            fontSize: cW * 0.03,
            fontWeight: 600,
            textAlign: 'center',
            background: `radial-gradient(circle, ${alpha(accent, 0.45)} 0%, #0f0f12 75%)`,
          }}
        >
          Add a screenshot
        </div>
      )}
    </div>
  )
}
