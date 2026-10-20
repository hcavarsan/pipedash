import {
  type Icon,
  IconCircleCaretRightFilled,
  IconCircleCheckFilled,
  IconCircleChevronsRightFilled,
  IconCircleRectangleFilled,
  IconCircleXFilled,
  IconClockFilled,
} from '@tabler/icons-react'

import type { PipelineStatus } from '../types'

type StatusTone = 'success' | 'failed' | 'running' | 'pending' | 'neutral'

export const STATUS_META: Record<PipelineStatus, { label: string; tone: StatusTone; icon: Icon }> = {
  success: { label: 'Success', tone: 'success', icon: IconCircleCheckFilled },
  failed: { label: 'Failed', tone: 'failed', icon: IconCircleXFilled },
  running: { label: 'Running', tone: 'running', icon: IconCircleCaretRightFilled },
  pending: { label: 'Pending', tone: 'pending', icon: IconClockFilled },
  cancelled: { label: 'Cancelled', tone: 'neutral', icon: IconCircleRectangleFilled },
  skipped: { label: 'Skipped', tone: 'neutral', icon: IconCircleChevronsRightFilled },
}

export const STATUS_OPTIONS = (Object.keys(STATUS_META) as PipelineStatus[]).map((status) => ({
  value: status,
  label: STATUS_META[status].label,
}))

const SEVERITY_ORDER: PipelineStatus[] = ['failed', 'running', 'pending', 'cancelled', 'success', 'skipped']

export const mostSevereStatus = (statuses: PipelineStatus[]): PipelineStatus | null =>
  SEVERITY_ORDER.find((status) => statuses.includes(status)) ?? null
