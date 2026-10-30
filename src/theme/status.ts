import {
  type Icon,
  IconAlertTriangle,
  IconBan,
  IconCircleCaretRightFilled,
  IconCircleCheck,
  IconCircleCheckFilled,
  IconCircleChevronsRight,
  IconCircleDashed,
  IconCircleXFilled,
  IconClockFilled,
  IconCloudOff,
  IconTool,
} from '@tabler/icons-react'

import type { HealthLevel, PipelineStatus } from '../types/domain'
import { typedKeys } from '../utils/typedKeys'

type StatusTone = 'success' | 'failed' | 'degraded' | 'running' | 'pending' | 'neutral'

export type StatusMeta = { label: string; tone: StatusTone; icon: Icon }

export const STATUS_META: Record<PipelineStatus, StatusMeta> = {
  success: { label: 'Success', tone: 'success', icon: IconCircleCheckFilled },
  failed: { label: 'Failed', tone: 'failed', icon: IconCircleXFilled },
  degraded: { label: 'Degraded', tone: 'degraded', icon: IconAlertTriangle },
  running: { label: 'Running', tone: 'running', icon: IconCircleCaretRightFilled },
  pending: { label: 'Pending', tone: 'pending', icon: IconClockFilled },
  cancelled: { label: 'Cancelled', tone: 'neutral', icon: IconBan },
  skipped: { label: 'Skipped', tone: 'neutral', icon: IconCircleChevronsRight },
}

export type StatusKey = PipelineStatus | 'never'

const NEVER_RUN: StatusMeta = { label: 'Never run', tone: 'neutral', icon: IconCircleDashed }

export const statusMeta = (status: StatusKey): StatusMeta => (status === 'never' ? NEVER_RUN : STATUS_META[status])

export const STATUS_OPTIONS = typedKeys(STATUS_META).map((status) => ({
  value: status,
  label: STATUS_META[status].label,
}))

export const HEALTH_META: Record<HealthLevel, { label: string; color: string; icon: Icon }> = {
  operational: { label: 'Operational', color: 'green', icon: IconCircleCheck },
  maintenance: { label: 'Maintenance', color: 'gray', icon: IconTool },
  degraded: { label: 'Degraded', color: 'orange', icon: IconAlertTriangle },
  partial_outage: { label: 'Partial outage', color: 'orange', icon: IconAlertTriangle },
  major_outage: { label: 'Major outage', color: 'orange', icon: IconCloudOff },
}
