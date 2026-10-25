import { project, rubberband } from './spring'

export type Axis = 'x' | 'y'

export interface Sample {
  t: number
  x: number
  y: number
}

export const AXIS_SLOP = 10
export const EDGE_ZONE = 24
export const PULL_THRESHOLD = 64
export const PULL_HOLD = 56

export const VELOCITY_WINDOW_MS = 100
const REVERSE_SPEED = 150
const FULL_SWIPE_RATIO = 0.6

export function lockAxis(dx: number, dy: number, slop = AXIS_SLOP): Axis | null {
  if (Math.max(Math.abs(dx), Math.abs(dy)) < slop) {
    return null
  }

  return Math.abs(dx) > Math.abs(dy) ? 'x' : 'y'
}

export function releaseVelocity(samples: Sample[], releaseTime: number): { vx: number; vy: number } {
  const recent = samples.filter((sample) => releaseTime - sample.t <= VELOCITY_WINDOW_MS)
  const first = recent[0]
  const last = recent[recent.length - 1]

  if (!first || !last || last.t <= first.t) {
    return { vx: 0, vy: 0 }
  }

  const seconds = (last.t - first.t) / 1000

  return { vx: (last.x - first.x) / seconds, vy: (last.y - first.y) / seconds }
}

export function resist(distance: number, dimension: number): number {
  return distance <= 0 ? 0 : rubberband(distance, dimension)
}

export function decideSwipeBack(dx: number, vx: number, width: number): 'complete' | 'cancel' {
  if (vx < -REVERSE_SPEED) {
    return 'cancel'
  }

  return project(dx, vx) > width / 2 ? 'complete' : 'cancel'
}

export function decideSheetDismiss(dy: number, vy: number, height: number): 'dismiss' | 'cancel' {
  if (vy < -REVERSE_SPEED) {
    return 'cancel'
  }

  return project(dy, vy) > height / 2 ? 'dismiss' : 'cancel'
}

export function nearestStop(position: number, velocity: number, stops: number[]): number {
  const projected = project(position, velocity)
  const ahead =
    Math.abs(velocity) > REVERSE_SPEED
      ? stops.filter((stop) => (velocity > 0 ? stop >= position : stop <= position))
      : stops
  const candidates = ahead.length > 0 ? ahead : stops

  return candidates.reduce((best, stop) => (Math.abs(stop - projected) < Math.abs(best - projected) ? stop : best))
}

export function decidePull(pull: number, threshold = PULL_THRESHOLD): 'refresh' | 'cancel' {
  return pull >= threshold ? 'refresh' : 'cancel'
}

export type RowSwipeResult = 'closed' | 'open' | 'full'

export const fullSwipeArmed = (position: number, rowWidth: number): boolean => position >= rowWidth * FULL_SWIPE_RATIO

export function decideRowSwipe(
  position: number,
  leftwardVelocity: number,
  actionsWidth: number,
  rowWidth: number
): RowSwipeResult {
  if (leftwardVelocity < -REVERSE_SPEED) {
    return 'closed'
  }

  if (fullSwipeArmed(position, rowWidth)) {
    return 'full'
  }

  return project(position, leftwardVelocity) > actionsWidth / 2 ? 'open' : 'closed'
}

export function rowPosition(raw: number, actionsWidth: number, rowWidth: number): number {
  if (raw < 0) {
    return rubberband(raw, actionsWidth)
  }

  return raw > rowWidth ? rowWidth + rubberband(raw - rowWidth, rowWidth) : raw
}
