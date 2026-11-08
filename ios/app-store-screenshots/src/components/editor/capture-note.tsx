import type { ReactNode } from 'react'
import { captureFits, DUO_FIT_TOLERANCE } from '@/lib/capture-fit'
import { CANVAS, DEVICE_LABEL } from '@/lib/device-specs'
import { duoGeometry, isDuoDevice } from '@/lib/frame-assets'
import type { Device } from '@/lib/types'
import { cn } from '@/lib/utils'

// iPhone Duo frames show a capture uncropped only when it came from that display.
export function CaptureNote({ device, src }: { device: Device; src: string }): ReactNode {
  if (!isDuoDevice(device)) {
    return null
  }
  const { w, h } = CANVAS[device]
  const fits = !src || captureFits(src, duoGeometry(device).screenAspect, DUO_FIT_TOLERANCE)
  const display = DEVICE_LABEL[device].replace(/ \(landscape\)$/, '')
  const orientationSuffix = device.endsWith('-landscape') ? ' in landscape' : ''
  const message = fits
    ? `Use a ${w} × ${h} capture from the ${display} display${orientationSuffix}.`
    : `This capture isn't ${w} × ${h} (${DEVICE_LABEL[device]}), so it is letterboxed, not cropped. Capture on that display.`
  return (
    <p className={cn('text-[11px] leading-relaxed', fits ? 'text-muted-foreground' : 'text-amber-600')}>{message}</p>
  )
}
