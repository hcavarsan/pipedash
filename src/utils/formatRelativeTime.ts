const RELATIVE_FORMAT = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

const UNITS: Array<[Intl.RelativeTimeFormatUnit, number]> = [
  ['day', 86_400_000],
  ['hour', 3_600_000],
  ['minute', 60_000],
]

export function formatRelativeTime(value: string | number | Date, now: number): string {
  const diff = new Date(value).getTime() - now

  for (const [unit, size] of UNITS) {
    if (Math.abs(diff) >= size) {
      return RELATIVE_FORMAT.format(Math.round(diff / size), unit)
    }
  }

  return RELATIVE_FORMAT.format(0, 'second')
}
