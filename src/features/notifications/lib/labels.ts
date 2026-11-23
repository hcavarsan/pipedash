import {
  type Icon,
  IconAlertTriangle,
  IconCircleCheck,
  IconCircleX,
  IconClock,
  IconCloudOff,
  IconPlugConnectedX,
} from '@tabler/icons-react'

import type { CenterNotification } from '../../../types/notifications'

export const TONE_COLORS: Record<CenterNotification['tone'], string> = {
  danger: 'var(--mantine-color-red-filled)',
  warning: 'var(--mantine-color-orange-filled)',
  success: 'var(--mantine-color-green-filled)',
}

export const noticeSubtitle = (label: string | undefined, time: string): string => (label ? `${label} · ${time}` : time)

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
