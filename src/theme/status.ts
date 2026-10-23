import {
  type Icon,
  IconAlertTriangle,
  IconCircleCaretRightFilled,
  IconCircleCheck,
  IconCircleCheckFilled,
  IconCircleChevronsRightFilled,
  IconCircleRectangleFilled,
  IconCircleXFilled,
  IconClockFilled,
  IconClockPause,
  IconCloudOff,
  IconPlugConnectedX,
  IconTool,
} from '@tabler/icons-react'

import type { HealthLevel, PipelineStatus, ProviderSummary, SignalKind } from '../types'
import type { AttentionTone } from '../utils/providerAttention'

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

export const HEALTH_META: Record<HealthLevel, { label: string; color: string; icon: Icon }> = {
  operational: { label: 'Operational', color: 'green', icon: IconCircleCheck },
  maintenance: { label: 'Maintenance', color: 'gray', icon: IconTool },
  degraded: { label: 'Degraded', color: 'orange', icon: IconAlertTriangle },
  partial_outage: { label: 'Partial outage', color: 'orange', icon: IconAlertTriangle },
  major_outage: { label: 'Major outage', color: 'orange', icon: IconCloudOff },
}

export const SIGNAL_ICONS: Partial<Record<SignalKind, Icon>> = {
  connection: IconPlugConnectedX,
  rate_limit: IconClockPause,
}

export const attentionColor = (summary: ProviderSummary, tone: AttentionTone): string =>
  tone === 'danger' ? 'red' : HEALTH_META[tone === 'info' ? 'maintenance' : summary.health.level].color
