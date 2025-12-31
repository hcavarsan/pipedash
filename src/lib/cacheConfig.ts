export const STALE_TIMES = {
  FAST_CHANGING: 30 * 1000,
  MODERATE: 60 * 1000,
  SLOW_CHANGING: 5 * 60 * 1000,
  STATIC: Infinity,
} as const

export const GC_TIMES = {
  SHORT: 5 * 60 * 1000,
  MEDIUM: 15 * 60 * 1000,
  LONG: 60 * 60 * 1000,
} as const
