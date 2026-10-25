export interface PlotSpan {
  left: number
  width: number
}

export const nearestIndex = (x: number, { left, width }: PlotSpan, count: number, band: boolean): number | null => {
  if (count <= 0 || width <= 0) {
    return null
  }

  const ratio = Math.min(1, Math.max(0, (x - left) / width))
  const index = band ? Math.floor(ratio * count) : Math.round(ratio * (count - 1))

  return Math.min(count - 1, index)
}
