import { captureFits, DUO_FIT_TOLERANCE } from './capture-fit'
import { slideNeedsScreenshot } from './export-filenames'
import { DUO_COMPANION, duoGeometry, isDuoDevice } from './frame-assets'
import { resolveScreenshot } from './locale'
import type { Device, ProjectState, Slide } from './types'

type PreflightWarning = { title: string; description: string }

const plural = (n: number) => (n === 1 ? '' : 's')

// A capture from another display is letterboxed inside a Duo frame, never
// cropped or stretched, so name it rather than let it pass unnoticed.
function fitsDisplay(raw: string | undefined, display: Device, locales: string[]): boolean {
  if (!raw || !isDuoDevice(display)) {
    return true
  }
  const aspect = duoGeometry(display).screenAspect
  return locales.every((loc) => captureFits(resolveScreenshot(raw, loc), aspect, DUO_FIT_TOLERANCE))
}

function countMismatched(state: ProjectState, slides: Slide[]): number {
  const device = state.device
  if (!isDuoDevice(device)) {
    return 0
  }
  const companion = DUO_COMPANION[device]
  return slides.filter(
    (slide) =>
      slideNeedsScreenshot(device, slide) &&
      (!fitsDisplay(slide.screenshot, device, state.locales) ||
        (slide.layout === 'two-devices' && !fitsDisplay(slide.screenshotSecondary, companion, state.locales)))
  ).length
}

/** What the user should know before exporting: empty devices, reused captures, mismatched Duo captures. */
export function exportPreflightWarning(state: ProjectState, slides: Slide[]): PreflightWarning | null {
  const device = state.device
  // On iPhone Duo the back device is the other display: it never reuses the
  // front capture, so a missing one leaves that device empty.
  const duoPair = (slide: Slide) => isDuoDevice(device) && slide.layout === 'two-devices'
  const missing = slides.filter(
    (slide) =>
      slideNeedsScreenshot(device, slide) && (!slide.screenshot || (duoPair(slide) && !slide.screenshotSecondary))
  ).length
  const reused = slides.filter(
    (slide) =>
      device !== 'feature-graphic' &&
      slide.layout === 'two-devices' &&
      !duoPair(slide) &&
      slide.screenshot &&
      !slide.screenshotSecondary
  ).length
  const mismatched = countMismatched(state, slides)
  if (missing === 0 && reused === 0 && mismatched === 0) {
    return null
  }
  const details = [
    missing ? `${missing} screen${plural(missing)} will export with an empty device.` : null,
    reused ? `${reused} two-device screen${plural(reused)} will reuse the primary screenshot in back.` : null,
    mismatched
      ? `${mismatched} screen${plural(mismatched)} use${mismatched === 1 ? 's' : ''} a capture that doesn't match its iPhone Duo display, so it is letterboxed. Capture on that display.`
      : null,
  ].filter(Boolean)
  return {
    title: missing || reused ? 'Export includes placeholder screenshots' : 'Check these screenshots',
    description: details.join(' '),
  }
}
