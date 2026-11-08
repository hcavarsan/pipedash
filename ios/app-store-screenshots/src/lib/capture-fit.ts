import { imgSize } from './image-cache'

// A capture whose aspect is off by more than this is letterboxed rather than
// cropped: cropping would cut real UI, which is worse than a visible bar.
// Loose enough for every size a device accepts: Apple Watch slots differ from
// the Ultra's 422 × 514 screen by up to 2.6 %.
const FIT_TOLERANCE = 0.03
// The two iPhone Duo displays differ by only 2.35 % (1398 × 2034 against
// 2007 × 2853), so Duo captures must match far more closely to tell an outer
// capture from an inner one. Scaled-down captures of the right display pass.
export const DUO_FIT_TOLERANCE = 0.005

export function captureFits(src: string, screenAspect: number, tolerance = FIT_TOLERANCE): boolean {
  const size = imgSize(src)
  return !size || Math.abs(size.w / size.h / screenAspect - 1) <= tolerance
}
