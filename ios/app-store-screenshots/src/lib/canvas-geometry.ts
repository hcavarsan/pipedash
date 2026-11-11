import type { CaptionRect, FrameComp, LayoutRects, Rect } from '@/components/editor/canvas-types'
import { getCanvas, getFrameAspect, getFrameForDevice } from './canvas-device'
import { getDefaultRects, getDuoRects } from './canvas-layout'
import { CREATIVE_SPECS, isCreative } from './device-specs'
import { imageElementKey, isImageElementId, isTextElementId, textElementKey } from './elements'
import { isDuoDevice } from './frame-assets'
import type { BuiltInElementId, Device, ElementId, ElementTransform, Orientation, Slide } from './types'

export function rectFor(id: BuiltInElementId, slide: Slide, defaults: LayoutRects): CaptionRect | undefined {
  const saved = slide.transforms?.[id]
  const def = defaults[id]
  if (!saved) {
    return def
  }
  return {
    x: saved.x,
    y: saved.y,
    width: saved.width,
    height: saved.height,
    align: def && 'align' in def ? def.align : undefined,
  }
}

// Devices whose frame must never be cropped by the canvas edge. A clipped TV,
// head unit, watch face or Mac window reads as a mistake, not as a deliberate bleed.
// A landscape phone hanging off the bottom reads as cropped, so Duo landscape
// targets are contained too.
const CONTAINED_DEVICES: Partial<Record<Device, true>> = {
  tvos: true,
  watchos: true,
  carplay: true,
  mac: true,
  'duo-outer-landscape': true,
  'duo-inner-landscape': true,
}

type SlideGeometry = {
  cW: number
  cH: number
  Frame: FrameComp
  frameAspect: number
  defaults: LayoutRects
  typeUnit: number | undefined
}

// The loupe starts overlapping the device's upper right, inside the screen.
function defaultCalloutRect(device: Rect, cW: number, cH: number, unit = Math.min(cW, cH)): Rect {
  const size = unit * 0.44
  return {
    x: Math.min(cW - size - cW * 0.04, device.x + device.width * 0.58),
    y: Math.max(cH * 0.04, Math.min(cH - size * 1.04, device.y + device.height * 0.16)),
    width: size,
    height: size,
  }
}

export function getSlideGeometry(slide: Slide, device: Device, orientation: Orientation): SlideGeometry {
  const { cW, cH } = getCanvas(device, orientation)
  const { Comp: Frame, widthFn, smallWidthFn } = getFrameForDevice(device, orientation)
  const frameAspect = getFrameAspect(device, orientation)
  if (isCreative(device)) {
    // Creatives compose inside Apple's art safe area, as if it were the canvas:
    // copy and focal device stay clear of every placement's crop while the
    // backdrop fills the whole asset. Devices are contained, so the focal
    // screen is never trimmed by a placement either.
    const { safe } = CREATIVE_SPECS[device]
    const local = getDefaultRects(
      slide.layout,
      safe.w,
      safe.h,
      frameAspect,
      widthFn(safe.w, safe.h),
      smallWidthFn(safe.w, safe.h),
      true
    )
    const defaults: LayoutRects = Object.fromEntries(
      Object.entries(local).map(([id, rect]) => [id, { ...rect, x: rect.x + safe.x, y: rect.y + safe.y }])
    )
    if (slide.callout && defaults.device) {
      defaults.callout = defaultCalloutRect(defaults.device, cW, cH, Math.min(safe.w, safe.h))
    }
    return { cW, cH, Frame, frameAspect, defaults, typeUnit: Math.min(safe.w, safe.h) * 1.15 }
  }
  const fwFrac = widthFn(cW, cH)
  const fwSmallFrac = smallWidthFn(cW, cH)
  const duo = isDuoDevice(device) ? getDuoRects(slide.layout, device, cW, cH) : null
  const defaults =
    duo || getDefaultRects(slide.layout, cW, cH, frameAspect, fwFrac, fwSmallFrac, CONTAINED_DEVICES[device] === true)
  if (slide.callout && defaults.device) {
    defaults.callout = defaultCalloutRect(defaults.device, cW, cH)
  }
  // Duo landscape copy runs across the top as one line, so it can be a size smaller.
  const typeUnit = duo && cW > cH ? cH * 0.86 : undefined
  return { cW, cH, Frame, frameAspect, defaults, typeUnit }
}

// Share of a frame's width taken by its screen. Close enough for every frame
// that the callout's zoom reads as "× the size it appears on the device".
export const SCREEN_WIDTH_FRACTION = 0.9

export function calloutAvailable(slide: Slide, device: Device): boolean {
  return device !== 'feature-graphic' && slide.layout !== 'no-device' && slide.layout !== 'feature-graphic'
}

export function defaultElementZ(id: BuiltInElementId): number {
  if (id === 'deviceSecondary') {
    return 2
  }
  if (id === 'device') {
    return 3
  }
  if (id === 'callout') {
    return 5
  }
  return 4
}

export function isFeatureGraphic(slide: Slide, device: Device): boolean {
  return slide.layout === 'feature-graphic' || device === 'feature-graphic'
}

export function getElementTransform(
  slide: Slide,
  device: Device,
  orientation: Orientation,
  id: ElementId
): ElementTransform | undefined {
  if (isTextElementId(id)) {
    const textId = textElementKey(id)
    return slide.textElements?.find((element) => element.id === textId)?.transform
  }
  if (isImageElementId(id)) {
    const imageId = imageElementKey(id)
    return slide.imageElements?.find((element) => element.id === imageId)?.transform
  }
  const { defaults } = getSlideGeometry(slide, device, orientation)
  const rect = rectFor(id, slide, defaults)
  if (!rect) {
    return undefined
  }
  const saved = slide.transforms?.[id]
  return {
    x: rect.x,
    y: rect.y,
    width: rect.width,
    height: rect.height,
    rotation: saved?.rotation ?? 0,
    zIndex: saved?.zIndex ?? defaultElementZ(id),
  }
}
