const UNITS = ['B', 'KB', 'MB', 'GB'] as const

export function formatBytes(n: number): string {
  let value = Number.isFinite(n) ? Math.max(0, n) : 0
  let unit = 0

  while (unit < UNITS.length - 1 && Math.round(value * 10) / 10 >= 1024) {
    value /= 1024
    unit += 1
  }

  return unit === 0 ? `${Math.round(value)} B` : `${value.toFixed(1)} ${UNITS[unit]}`
}
