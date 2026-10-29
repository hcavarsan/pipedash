import { rubberband } from './spring'

export const SHEET_DETENTS = ['medium', 'large'] as const
export type SheetDetent = (typeof SHEET_DETENTS)[number]

export const MEDIUM_RATIO = 0.5
export const FLICK_SPEED = 500

export interface SheetFrame {
  height: number
  offset: number
}

const clamp = (value: number, min: number, max: number): number => Math.min(max, Math.max(min, value))

export function detentHeights(detents: readonly SheetDetent[], available: number): number[] {
  const heights = detents.map((detent) => (detent === 'large' ? available : Math.round(available * MEDIUM_RATIO)))

  return [...new Set(heights)].sort((a, b) => a - b)
}

export function stopPositions(heights: number[], dismissible: boolean): number[] {
  const top = heights.at(-1) ?? 0
  const stops = heights.map((height) => top - height).reverse()

  return dismissible ? [...stops, top] : stops
}

export function displayPosition(raw: number, top: number, floor: number): number {
  if (raw < 0) {
    return -rubberband(-raw, top)
  }

  return raw > floor ? floor + rubberband(raw - floor, top) : raw
}

export function sheetFrame(position: number, heights: number[]): SheetFrame {
  const [low = 0] = heights
  const height = (heights.at(-1) ?? 0) - position

  return height >= low ? { height, offset: 0 } : { height: low, offset: low - height }
}

export function scrimLevel({ offset }: SheetFrame, heights: number[]): number {
  const [low = 0] = heights

  return clamp(1 - offset / low, 0, 1)
}

export function expandLevel(height: number, heights: number[]): number | null {
  if (heights.length < 2) {
    return null
  }

  const [low = 0] = heights

  return clamp((height - low) / ((heights.at(-1) ?? 0) - low), 0, 1)
}
