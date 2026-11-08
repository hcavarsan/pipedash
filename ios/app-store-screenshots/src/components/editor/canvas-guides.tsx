import type { ReactNode } from 'react'
import { CREATIVE_SPECS, centredCrop, isCreative } from '@/lib/device-specs'
import type { Device } from '@/lib/types'

function GuideTag({
  text,
  color,
  line,
  cW,
  outside = false,
}: {
  text: string
  color: string
  line: number
  cW: number
  outside?: boolean
}): ReactNode {
  return (
    <span
      style={{
        position: 'absolute',
        left: 0,
        top: 0,
        transform: outside ? 'translateY(-100%)' : undefined,
        background: color,
        color: 'white',
        fontSize: Math.max(24, cW * 0.0075),
        fontWeight: 700,
        lineHeight: 1,
        padding: `${line * 2}px ${line * 3}px`,
        whiteSpace: 'nowrap',
        fontFamily: 'ui-sans-serif, system-ui, sans-serif',
      }}
    >
      {text}
    </span>
  )
}

// Editor-only overlay (never exported): Apple's art safe area, plus how the
// universal asset may be cropped in each placement.
export function CreativeGuide({
  device,
  cW,
  cH,
  left,
}: {
  device: Device
  cW: number
  cH: number
  left: number
}): ReactNode {
  if (!isCreative(device)) {
    return null
  }
  const { safe, crops } = CREATIVE_SPECS[device]
  const line = Math.max(3, cW * 0.0012)
  return (
    <div
      aria-hidden
      data-creative-guide
      style={{ position: 'absolute', left, top: 0, width: cW, height: cH, pointerEvents: 'none', zIndex: 1000 }}
    >
      {crops.map((crop) => {
        const r = centredCrop(cW, cH, crop.aspect)
        return (
          <div
            key={crop.label}
            style={{
              position: 'absolute',
              left: r.x,
              top: r.y,
              width: r.w,
              height: r.h,
              outline: `${line}px dashed rgba(250, 204, 21, 0.9)`,
              outlineOffset: -line,
            }}
          >
            <GuideTag text={`${crop.label} (estimate)`} color="rgba(161, 98, 7, 0.9)" line={line} cW={cW} />
          </div>
        )
      })}
      <div
        style={{
          position: 'absolute',
          left: safe.x,
          top: safe.y,
          width: safe.w,
          height: safe.h,
          outline: `${line}px solid rgba(34, 197, 94, 0.95)`,
          outlineOffset: -line,
          background: 'rgba(34, 197, 94, 0.06)',
        }}
      >
        <GuideTag text="Art safe area" color="rgba(21, 128, 61, 0.95)" line={line} cW={cW} outside />
      </div>
    </div>
  )
}

export function ScreenGuide({
  cW,
  cH,
  index,
  active,
}: {
  cW: number
  cH: number
  index: number
  active: boolean
}): ReactNode {
  const outline = active ? Math.max(4, cW * 0.003) : Math.max(2, cW * 0.0015)
  return (
    <div
      aria-hidden
      style={{
        position: 'absolute',
        inset: 0,
        pointerEvents: 'none',
        outline: `${outline}px solid ${active ? 'rgba(91, 124, 250, 0.95)' : 'rgba(15, 23, 42, 0.22)'}`,
        outlineOffset: -outline,
        boxShadow: active ? 'inset 0 0 0 9999px rgba(91, 124, 250, 0.03)' : 'inset 0 0 0 1px rgba(255, 255, 255, 0.22)',
      }}
    >
      <div
        style={{
          position: 'absolute',
          left: cW * 0.035,
          top: cH * 0.024,
          borderRadius: cW * 0.018,
          padding: `${cH * 0.006}px ${cW * 0.018}px`,
          background: active ? 'rgba(91, 124, 250, 0.92)' : 'rgba(15, 23, 42, 0.72)',
          color: 'white',
          fontSize: Math.max(24, cW * 0.022),
          lineHeight: 1,
          fontWeight: 700,
          letterSpacing: 0,
        }}
      >
        {index + 1}
      </div>
    </div>
  )
}
