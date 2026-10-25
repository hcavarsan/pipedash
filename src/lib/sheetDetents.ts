import { rubberband } from './spring'

export type SheetDetent = 'medium' | 'large'

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
  const top = heights[heights.length - 1]
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
  const height = heights[heights.length - 1] - position

  return height >= heights[0] ? { height, offset: 0 } : { height: heights[0], offset: heights[0] - height }
}

export function scrimLevel({ offset }: SheetFrame, heights: number[]): number {
  return clamp(1 - offset / heights[0], 0, 1)
}

export function expandLevel(height: number, heights: number[]): number | null {
  if (heights.length < 2) {
    return null
  }

  return clamp((height - heights[0]) / (heights[heights.length - 1] - heights[0]), 0, 1)
}
