import type { CaptionRect, LayoutRects } from '@/components/editor/canvas-types'
import { CANVAS } from './device-specs'
import { DUO_COMPANION, type DuoDevice, duoGeometry } from './frame-assets'
import type { Slide } from './types'

// ---------- Default element rects per layout ----------

export function getDefaultRects(
  layout: Slide['layout'],
  cW: number,
  cH: number,
  frameAspect: number,
  fwFrac: number,
  fwSmallFrac: number,
  // Phones and tablets are deliberately hung past the canvas edge so they bleed off
  // it. When true (TV, watch, CarPlay, Mac), every device rect stays fully inside the
  // canvas instead.
  contain = false
): LayoutRects {
  const deviceW = fwFrac * cW
  const deviceH = deviceW / frameAspect
  const smallW = fwSmallFrac * cW
  const smallH = smallW / frameAspect
  const capW = cW * 0.84
  const capH = cH * 0.28

  switch (layout) {
    case 'hero':
      return {
        caption: { x: cW * 0.08, y: cH * 0.09, width: capW, height: capH, align: 'center' },
        device: {
          x: (cW - deviceW) / 2,
          y: contain ? cH - deviceH - cH * 0.05 : cH - deviceH + deviceH * 0.15,
          width: deviceW,
          height: deviceH,
        },
      }
    case 'device-bottom':
      return {
        caption: { x: cW * 0.08, y: cH * 0.08, width: capW, height: capH, align: 'center' },
        device: {
          x: (cW - deviceW) / 2,
          y: cH - deviceH - cH * 0.02,
          width: deviceW,
          height: deviceH,
        },
      }
    case 'device-top':
      return {
        caption: { x: cW * 0.08, y: cH * 0.65, width: capW, height: capH, align: 'center' },
        device: {
          x: (cW - deviceW) / 2,
          y: contain ? cH * 0.05 : -cH * 0.1,
          width: deviceW,
          height: deviceH,
        },
      }
    case 'two-devices':
      return {
        caption: { x: cW * 0.08, y: cH * 0.08, width: capW, height: capH, align: 'center' },
        deviceSecondary: {
          x: contain ? cW * 0.04 : -cW * 0.06,
          y: cH - smallH - cH * 0.05,
          width: smallW,
          height: smallH,
        },
        device: {
          x: contain ? cW - deviceW * 0.9 - cW * 0.04 : cW - deviceW * 0.9 + cW * 0.06,
          y: contain ? cH - (deviceW * 0.9) / frameAspect - cH * 0.05 : cH - deviceH * 0.9 - cH * 0.02,
          width: deviceW * 0.9,
          height: (deviceW * 0.9) / frameAspect,
        },
      }
    case 'no-device':
      return {
        caption: {
          x: cW * 0.1,
          y: cH * 0.35,
          width: cW * 0.8,
          height: cH * 0.3,
          align: 'center',
        },
      }
    case 'split-landscape': {
      // Contained devices sit beside the caption instead of sliding under it.
      // The caption is beside it, not above, so it can be taller than elsewhere.
      const splitW = contain ? Math.min(cW * 0.5, cH * 0.84 * frameAspect) : deviceW
      const splitH = splitW / frameAspect
      return {
        caption: {
          x: cW * 0.05,
          y: cH * 0.25,
          width: cW * 0.38,
          height: cH * 0.5,
          align: 'left',
        },
        device: {
          x: contain ? cW - splitW - cW * 0.05 : cW - deviceW + cW * 0.03,
          y: (cH - splitH) / 2,
          width: splitW,
          height: splitH,
        },
      }
    }
    default:
      return {}
  }
}

// ---------- iPhone Duo placements ----------
//
// Duo decks get their own placements, modelled on the first published Duo sets
// and Apple's guidance (see iphone-duo.md in the skill):
// - Landscape canvases (inner display opened, outer turned) are about as wide as
//   an iPad. A short headline runs across the top and the device is the hero,
//   wide enough that the capture reads, rather than a phone-sized device beside
//   a narrow column of copy.
// - "Two devices" becomes "Folded + open": the second device is the same phone in
//   its other state, drawn in that display's frame. Opening an upright closed
//   phone gives the inner display in landscape, at the same height, so that pair
//   is drawn to physical scale side by side.
// Portrait decks keep the phone placements from getDefaultRects.

/** Width of the companion frame that matches a primary frame `primaryW` wide, to physical scale. */
function physicalCompanionW(device: DuoDevice, primaryW: number): number {
  const companion = DUO_COMPANION[device]
  const p = duoGeometry(device)
  const c = duoGeometry(companion)
  // Both displays are captured at the same pixel density, so capture pixels scale alike.
  const canvasPerPixel = (primaryW * p.screen.W) / 100 / CANVAS[device].w
  return (canvasPerPixel * CANVAS[companion].w) / (c.screen.W / 100)
}

function duoTwoDevices(device: DuoDevice, cW: number, cH: number, topCaption: CaptionRect): LayoutRects {
  const fa = duoGeometry(device).aspect
  const ca = duoGeometry(DUO_COMPANION[device]).aspect
  if (cW > cH) {
    // Side by side, bottom-aligned: the companion leads on the left.
    const gap = cW * 0.03
    const k = physicalCompanionW(device, 1)
    // Folded and open heights match for the upright pair; otherwise keep the
    // taller companion from crowding the headline.
    const w = Math.min((cW * 0.9 - gap) / (1 + k), cH * 0.66 * fa, (cH * 0.7 * ca) / k)
    const cw = w * k
    const x0 = (cW - (w + gap + cw)) / 2
    const bottom = cH * 0.95
    return {
      caption: topCaption,
      deviceSecondary: { x: x0, y: bottom - cw / ca, width: cw, height: cw / ca },
      device: { x: x0 + cw + gap, y: bottom - w / fa, width: w, height: w / fa },
    }
  }
  if (device === 'duo-outer') {
    // The opened phone sits behind, upper left; the closed one in front, lower right.
    const cw = cW * 0.86
    const w = Math.min(cW * 0.46, cH * 0.42 * fa)
    return {
      caption: topCaption,
      deviceSecondary: { x: cW * 0.04, y: cH * 0.27, width: cw, height: cw / ca },
      device: { x: cW - w - cW * 0.05, y: cH * 0.97 - w / fa, width: w, height: w / fa },
    }
  }
  // Inner portrait: the closed phone is short and wide, so the pair stands side
  // by side, bottom-aligned, with the open display leading in size.
  const gap = cW * 0.03
  const cw = cW * 0.3
  const w = cW * 0.62
  const x0 = (cW - (cw + gap + w)) / 2
  const bottom = cH * 0.95
  return {
    caption: topCaption,
    deviceSecondary: { x: x0, y: bottom - cw / ca, width: cw, height: cw / ca },
    device: { x: x0 + cw + gap, y: bottom - w / fa, width: w, height: w / fa },
  }
}

export function getDuoRects(layout: Slide['layout'], device: DuoDevice, cW: number, cH: number): LayoutRects | null {
  const fa = duoGeometry(device).aspect
  const landscape = cW > cH
  const topCaption = {
    x: cW * 0.06,
    y: cH * 0.06,
    width: cW * 0.88,
    height: cH * (landscape ? 0.18 : 0.2),
    align: 'center' as const,
  }

  if (layout === 'two-devices') {
    return duoTwoDevices(device, cW, cH, topCaption)
  }

  if (!landscape) {
    return null
  }
  switch (layout) {
    case 'hero': {
      const w = cW * 0.82
      return { caption: topCaption, device: { x: (cW - w) / 2, y: cH * 0.31, width: w, height: w / fa } }
    }
    case 'device-bottom': {
      const h = cH * 0.64
      return { caption: topCaption, device: { x: (cW - h * fa) / 2, y: cH * 0.96 - h, width: h * fa, height: h } }
    }
    case 'device-top': {
      const w = cW * 0.82
      const h = w / fa
      return {
        caption: { ...topCaption, y: cH * 0.74 },
        device: { x: (cW - w) / 2, y: cH * 0.69 - h, width: w, height: h },
      }
    }
    case 'split-landscape': {
      const w = Math.min(cW * 0.58, cH * 0.86 * fa)
      return {
        caption: { x: cW * 0.05, y: cH * 0.22, width: cW * 0.33, height: cH * 0.56, align: 'left' },
        device: { x: cW - w - cW * 0.04, y: (cH - w / fa) / 2, width: w, height: w / fa },
      }
    }
    default:
      return null
  }
}
