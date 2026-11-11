export const RETENTION_PRESETS = [
  ...[7, 14, 30, 60, 90].map((days) => ({ value: String(days), label: `${days} days` })),
  { value: 'custom', label: 'Custom' },
]
