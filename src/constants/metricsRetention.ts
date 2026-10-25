export const RETENTION_PRESETS = [
  ...[7, 14, 30, 60, 90].map((days) => ({ value: String(days), label: `${days} days` })),
  { value: 'custom', label: 'Custom' },
]

export function validateRetentionDays(value: string | number): string | null {
  const days = typeof value === 'string' && value.trim() === '' ? Number.NaN : Number(value)

  return Number.isInteger(days) && days >= 1 && days <= 90 ? null : 'Enter a whole number from 1 to 90.'
}
