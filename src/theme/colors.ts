import type { MantineColor } from '@mantine/core'

import type { PipelineStatus } from '../types'

export const STATUS_COLORS: Record<PipelineStatus, MantineColor> = {
  success: 'blue',
  failed: 'red',
  running: 'cyan',
  pending: 'yellow',
  cancelled: 'gray',
  skipped: 'gray',
} as const

