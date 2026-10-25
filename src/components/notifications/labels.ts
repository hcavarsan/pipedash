import {
  type Icon,
  IconAlertTriangle,
  IconCircleCheck,
  IconCircleX,
  IconClock,
  IconCloudOff,
  IconPlugConnectedX,
} from '@tabler/icons-react'

import type { CenterNotification } from '../../stores/notificationCenterStore'
import { HEALTH_META } from '../../theme/status'

export const TONE_COLORS: Record<CenterNotification['tone'], string> = {
  danger: 'var(--mantine-color-red-filled)',
  warning: 'var(--mantine-color-orange-filled)',
  success: 'var(--mantine-color-green-filled)',
}

export function notificationLabel({ tone, level, pipelineId }: CenterNotification): string {
  if (tone === 'success') {
    return 'Resolved'
  }

  if (pipelineId) {
    return tone === 'danger' ? 'Pipeline failed' : 'Running longer than usual'
  }

  return level ? HEALTH_META[level].label : 'Notice'
}

export function notificationIcon({ tone, level, key, pipelineId }: CenterNotification): Icon {
  if (tone === 'success') {
    return IconCircleCheck
  }

  if (tone === 'danger') {
    return IconCircleX
  }

  if (pipelineId) {
    return IconClock
  }

  if (key.startsWith('connection:')) {
    return IconPlugConnectedX
  }

  return level === 'major_outage' ? IconCloudOff : IconAlertTriangle
}
