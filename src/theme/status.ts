import type { MantineColor } from '@mantine/core'
import { type Icon, IconCheck, IconCircleOff, IconClock, IconLoader, IconMinus, IconX } from '@tabler/icons-react'

import type { PipelineStatus } from '../types'

export const STATUS_META: Record<PipelineStatus, { label: string; color: MantineColor; icon: Icon }> = {
  success: { label: 'Success', color: 'green', icon: IconCheck },
  failed: { label: 'Failed', color: 'red', icon: IconX },
  running: { label: 'Running', color: 'blue', icon: IconLoader },
  pending: { label: 'Pending', color: 'yellow', icon: IconClock },
  cancelled: { label: 'Cancelled', color: 'gray', icon: IconCircleOff },
  skipped: { label: 'Skipped', color: 'gray', icon: IconMinus },
}

export const STATUS_OPTIONS = (Object.keys(STATUS_META) as PipelineStatus[]).map((status) => ({
  value: status,
  label: STATUS_META[status].label,
}))

const SEVERITY_ORDER: PipelineStatus[] = ['failed', 'running', 'pending', 'cancelled', 'success', 'skipped']

export const mostSevereStatus = (statuses: PipelineStatus[]): PipelineStatus | null =>
  SEVERITY_ORDER.find((status) => statuses.includes(status)) ?? null
