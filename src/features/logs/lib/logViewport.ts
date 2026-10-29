export const WINDOW_ROWS = 100_000
export const MAX_SCROLL_PX = 8_000_000
export const OVERSCAN_ROWS = 30

const REBASE_EDGE = 0.15

export interface RowWindow {
  base: number
  size: number
}

export function planWindow(rowCount: number, anchorRow: number, heightOf: (row: number) => number): RowWindow {
  if (rowCount <= 0) {
    return { base: 0, size: 0 }
  }

  const anchor = Math.min(Math.max(anchorRow, 0), rowCount - 1)
  let low = anchor
  let high = anchor + 1
  let total = heightOf(anchor)
  let up = true

  while (high - low < WINDOW_ROWS) {
    const canUp = low > 0
    const canDown = high < rowCount

    if (!canUp && !canDown) {
      break
    }

    const goUp = canUp && (up || !canDown)
    const height = heightOf(goUp ? low - 1 : high)

    if (total + height >= MAX_SCROLL_PX) {
      break
    }

    total += height

    if (goUp) {
      low -= 1
    } else {
      high += 1
    }

    up = !up
  }

  return { base: low, size: high - low }
}

export function needsRebase(base: number, size: number, rowCount: number, firstVisible: number): boolean {
  const relative = firstVisible - base

  if (relative < size * REBASE_EDGE) {
    return base > 0
  }

  if (relative >= size * (1 - REBASE_EDGE)) {
    return base + size < rowCount
  }

  return false
}
