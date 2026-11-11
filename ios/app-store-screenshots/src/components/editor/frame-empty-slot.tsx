'use client'
import type { ReactNode } from 'react'
import { img } from '@/lib/image-cache'
import { PlainImg } from './plain-img'

export function EmptySlot(): ReactNode {
  // Sized off the slot's own width (container query units) so the hint stays
  // readable on a 4K TV canvas and still fits inside a 422 px watch face.
  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        containerType: 'inline-size',
        background: 'linear-gradient(135deg, #1a1a1a 0%, #0a0a0a 100%)',
      }}
    >
      <div
        style={{
          width: '100%',
          height: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: 'rgba(255,255,255,0.4)',
          fontSize: '6cqw',
          textAlign: 'center',
          padding: '8%',
          boxSizing: 'border-box',
        }}
      >
        Drop a screenshot here
      </div>
    </div>
  )
}

type ScreenImageProps = {
  src: string
  alt: string
  hideEmpty: boolean | undefined
  objectFit?: 'cover' | 'contain'
  objectPosition?: 'top' | 'center'
}

/** The capture filling a frame's screen, or the drop hint when there is none. */
export function ScreenImage({
  src,
  alt,
  hideEmpty,
  objectFit = 'cover',
  objectPosition = 'top',
}: ScreenImageProps): ReactNode {
  const resolved = img(src)
  if (resolved) {
    return (
      <PlainImg
        src={resolved}
        alt={alt}
        style={{ display: 'block', width: '100%', height: '100%', objectFit, objectPosition }}
        draggable={false}
      />
    )
  }
  return hideEmpty ? null : <EmptySlot />
}
