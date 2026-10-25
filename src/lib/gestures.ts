export type Axis = 'x' | 'y'

export interface Sample {
  t: number
  x: number
  y: number
}

export const AXIS_SLOP = 8
export const EDGE_ZONE = 24
export const PULL_THRESHOLD = 50
export const PULL_HOLD = 50
export const PULL_LIMIT = 120

const VELOCITY_WINDOW_MS = 100
const FLICK = 0.5

export function lockAxis(dx: number, dy: number, slop = AXIS_SLOP): Axis | null {
  if (Math.max(Math.abs(dx), Math.abs(dy)) < slop) {
    return null
  }

  return Math.abs(dx) > Math.abs(dy) ? 'x' : 'y'
}

export function releaseVelocity(samples: Sample[]): { vx: number; vy: number } {
  const last = samples[samples.length - 1]

  if (!last) {
    return { vx: 0, vy: 0 }
  }

  const first = samples.find((sample) => last.t - sample.t <= VELOCITY_WINDOW_MS) ?? last
  const dt = last.t - first.t

  if (dt <= 0) {
    return { vx: 0, vy: 0 }
  }

  return { vx: (last.x - first.x) / dt, vy: (last.y - first.y) / dt }
}

export function rubberBand(distance: number, limit: number, stiffness = 1): number {
  if (distance <= 0) {
    return 0
  }

  return (1 - 1 / ((distance * stiffness) / limit + 1)) * limit
}

export function decideSwipeBack(dx: number, vx: number, width: number): 'complete' | 'cancel' {
  if (vx < -FLICK / 2) {
    return 'cancel'
  }

  return dx > width * 0.4 || (vx > FLICK && dx > 16) ? 'complete' : 'cancel'
}

export function decideSheetDismiss(dy: number, vy: number, height: number): 'dismiss' | 'cancel' {
  if (vy < -FLICK / 2) {
    return 'cancel'
  }

  return dy > height * 0.35 || (vy > FLICK && dy > 24) ? 'dismiss' : 'cancel'
}

export function decidePull(pull: number, threshold = PULL_THRESHOLD): 'refresh' | 'cancel' {
  return pull >= threshold ? 'refresh' : 'cancel'
}

export type RowSwipeResult = 'closed' | 'open' | 'full'

export function decideRowSwipe(position: number, leftwardVelocity: number, actionsWidth: number, rowWidth: number) {
  if (position > rowWidth * 0.6 || (leftwardVelocity > FLICK * 2 && position > actionsWidth * 1.15)) {
    return 'full' satisfies RowSwipeResult
  }

  if (leftwardVelocity > FLICK / 2) {
    return 'open' satisfies RowSwipeResult
  }

  if (leftwardVelocity < -FLICK / 2) {
    return 'closed' satisfies RowSwipeResult
  }

  return (position > actionsWidth / 2 ? 'open' : 'closed') satisfies RowSwipeResult
}

export function rowPosition(raw: number, actionsWidth: number, rowWidth: number): number {
  if (raw < 0) {
    return -rubberBand(-raw, actionsWidth)
  }

  return Math.min(raw, rowWidth)
}
