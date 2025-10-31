
const COLUMN_WIDTHS = {
  xs: 80,
  sm: 100,
  md: 140,
  lg: 180,
  xl: 240,
  flex: undefined,
} as const


export const COLUMN_PRESETS = {
  identifier: {
    width: COLUMN_WIDTHS.sm,
    textAlign: 'left' as const,
    ellipsis: false,
  },

  status: {
    width: COLUMN_WIDTHS.md,
    textAlign: 'left' as const,
    ellipsis: false,
  },

  timestamp: {
    width: COLUMN_WIDTHS.lg,
    textAlign: 'left' as const,
    ellipsis: false,
  },

  branch: {
    width: COLUMN_WIDTHS.flex,
    minWidth: 160,
    maxWidth: 320,
    textAlign: 'left' as const,
    ellipsis: true,
  },

  name: {
    width: COLUMN_WIDTHS.flex,
    minWidth: 180,
    maxWidth: 350,
    textAlign: 'left' as const,
    ellipsis: true,
  },

  actions: {
    width: COLUMN_WIDTHS.xs,
    textAlign: 'center' as const,
    ellipsis: false,
  },

  commit: {
    width: 160,
    textAlign: 'left' as const,
    ellipsis: false,
  },

  duration: {
    width: 120,
    textAlign: 'center' as const,
    ellipsis: false,
  },

  count: {
    width: COLUMN_WIDTHS.md,
    textAlign: 'center' as const,
    ellipsis: false,
  },

  organization: {
    width: COLUMN_WIDTHS.lg,
    textAlign: 'left' as const,
    ellipsis: true,
  },

  providers: {
    width: 250,
    textAlign: 'left' as const,
    ellipsis: false,
  },
} as const

