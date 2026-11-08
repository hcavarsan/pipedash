import { bezelFrameFor, DUO_FRAMES } from '@/components/editor/bezel-frame'
import type { FrameComp } from '@/components/editor/canvas-types'
import { AppleTV, AppleWatch, CarPlayScreen, MacWindow } from '@/components/editor/desktop-frames'
import { AndroidPhone, Phone } from '@/components/editor/phone-frames'
import { AndroidTabletL, AndroidTabletP, IPad } from '@/components/editor/tablet-frames'
import {
  CANVAS,
  CARPLAY_RATIO,
  carPlayW,
  frameFitW,
  IPAD_RATIO,
  ipadW,
  isCreative,
  MAC_RATIO,
  MK_RATIO,
  macW,
  phoneW,
  phoneWSmall,
  TV_RATIO,
  tabletLW,
  tabletPW,
  tvW,
  WATCH_RATIO,
  watchW,
} from './device-specs'
import { bezelGeometry, type DuoDevice, duoGeometry, isDuoDevice } from './frame-assets'
import type { Device, Orientation } from './types'

type WidthFn = (cW: number, cH: number) => number

type FrameFit = { Comp: FrameComp; widthFn: WidthFn; smallWidthFn: WidthFn }

export function getCanvas(device: Device, orientation: Orientation): { cW: number; cH: number } {
  const c = CANVAS[device]
  if ((device === 'android-7' || device === 'android-10') && orientation === 'landscape') {
    if (c.wL !== undefined && c.hL !== undefined) {
      return { cW: c.wL, cH: c.hL }
    }
  }
  return { cW: c.w, cH: c.h }
}

const PORTRAIT_ASPECTS: Partial<Record<Device, number>> = {
  iphone: MK_RATIO,
  android: 9 / 19.5,
  ipad: IPAD_RATIO,
  tvos: TV_RATIO,
  watchos: WATCH_RATIO,
  carplay: CARPLAY_RATIO,
  mac: MAC_RATIO,
}

// Aspect ratio (w/h) of each device frame — must match the frame components
export function getFrameAspect(device: Device, orientation: Orientation): number {
  const bezel = bezelGeometry(device)
  if (bezel) {
    return bezel.aspect
  }
  if (isCreative(device)) {
    return MK_RATIO
  }
  if (device === 'android-7' || device === 'android-10') {
    return orientation === 'landscape' ? 8 / 5 : 5 / 8
  }
  return PORTRAIT_ASPECTS[device] ?? 1
}

type BezelFit = { height: number; clamp: number; small: number; smallHeight?: number }

const IPHONE_FIT: BezelFit = { height: 0.72, clamp: 0.84, small: 0.66 }
const CREATIVE_FIT: BezelFit = { height: 0.62, clamp: 0.9, small: 0.7, smallHeight: 0.5 }

// Height share and width clamps for a device under Apple's bezel; they match
// the built-in frame's width functions in device-specs.ts. The watch bezel
// includes its band, which takes nearly half its height; the band's top stays
// below a two-line headline.
const BEZEL_FIT: Partial<Record<Device, BezelFit>> = {
  iphone: IPHONE_FIT,
  ipad: { height: 0.72, clamp: 0.75, small: 0.6 },
  tvos: { height: 0.72, clamp: 0.58, small: 0.56 },
  watchos: { height: 0.66, clamp: 0.62, small: 0.5 },
  mac: { height: 0.58, clamp: 0.86, small: 0.46 },
  'creative-universal': CREATIVE_FIT,
  'creative-header': CREATIVE_FIT,
  'creative-search': CREATIVE_FIT,
}

function duoFrameFit(device: DuoDevice): FrameFit {
  // Sized from the frame's real aspect, so the squarer inner display gets a
  // wider device instead of a stretched iPhone placement.
  const aspect = duoGeometry(device).aspect
  if (aspect > 1) {
    return {
      Comp: DUO_FRAMES[device],
      widthFn: frameFitW(aspect, 0.58, 0.86),
      smallWidthFn: frameFitW(aspect, 0.46, 0.7),
    }
  }
  return {
    Comp: DUO_FRAMES[device],
    widthFn: frameFitW(aspect, 0.66, 0.84),
    smallWidthFn: frameFitW(aspect, 0.52, 0.66),
  }
}

function builtInFrameFit(device: Device, orientation: Orientation): FrameFit {
  switch (device) {
    case 'ipad':
      return { Comp: IPad, widthFn: ipadW, smallWidthFn: (cW, cH) => ipadW(cW, cH, 0.6) }
    case 'tvos':
      return { Comp: AppleTV, widthFn: tvW, smallWidthFn: (cW, cH) => tvW(cW, cH, 0.56) }
    case 'watchos':
      return { Comp: AppleWatch, widthFn: watchW, smallWidthFn: (cW, cH) => watchW(cW, cH, 0.42) }
    case 'carplay':
      return { Comp: CarPlayScreen, widthFn: carPlayW, smallWidthFn: (cW, cH) => carPlayW(cW, cH, 0.7) }
    case 'mac':
      return { Comp: MacWindow, widthFn: macW, smallWidthFn: (cW, cH) => macW(cW, cH, 0.46) }
    case 'android':
      return { Comp: AndroidPhone, widthFn: phoneW, smallWidthFn: phoneWSmall }
    case 'android-7':
    case 'android-10':
      if (orientation === 'landscape') {
        return { Comp: AndroidTabletL, widthFn: tabletLW, smallWidthFn: (cW, cH) => tabletLW(cW, cH, 0.5) }
      }
      return { Comp: AndroidTabletP, widthFn: tabletPW, smallWidthFn: (cW, cH) => tabletPW(cW, cH, 0.62) }
    default:
      return { Comp: Phone, widthFn: phoneW, smallWidthFn: phoneWSmall }
  }
}

export function getFrameForDevice(device: Device, orientation: Orientation): FrameFit {
  if (isDuoDevice(device)) {
    return duoFrameFit(device)
  }
  // Apple's own bezel, when the user has added it: the same placements as the
  // built-in frame, sized from the bezel's real aspect.
  const bezel = bezelGeometry(device)
  if (bezel) {
    const fit = BEZEL_FIT[device] ?? IPHONE_FIT
    return {
      Comp: bezelFrameFor(device),
      widthFn: frameFitW(bezel.aspect, fit.height, fit.clamp),
      smallWidthFn: frameFitW(bezel.aspect, fit.smallHeight ?? fit.height, fit.small),
    }
  }
  // Creatives lay out inside their safe area (see getSlideGeometry), so these
  // fractions are of the safe area, not the canvas.
  if (isCreative(device)) {
    return { Comp: Phone, widthFn: frameFitW(MK_RATIO, 0.62, 0.9), smallWidthFn: frameFitW(MK_RATIO, 0.5, 0.7) }
  }
  return builtInFrameFit(device, orientation)
}
