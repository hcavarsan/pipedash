import {
  type Icon,
  IconAlertTriangle,
  IconCircleCheck,
  IconCircleX,
  IconClock,
  IconCloudOff,
  IconPlugConnectedX,
  IconTool,
} from '@tabler/icons-react'

import type { CenterNotification } from '../../stores/notificationCenterStore'
import { HEALTH_META, SIGNAL_ICONS } from '../../theme/status'
import type { ProviderIssue } from '../../utils/providerAttention'

export function issueLabel({ tone, provider }: ProviderIssue): string {
  if (tone === 'danger') {
    return 'Fetch failed'
  }

  return tone === 'info' ? 'Maintenance' : HEALTH_META[provider.health.level].label
}

export function issueSummary(issue: ProviderIssue): string {
  const label = issueLabel(issue)
  const extra = issue.tone === 'danger' ? (issue.attention.hint ?? issue.attention.detail) : issue.headline

  return extra && extra !== label ? `${label} · ${extra}` : label
}

export function issueIcon({ tone, provider, attention }: ProviderIssue): Icon {
  if (tone === 'danger') {
    return IconCircleX
  }

  if (tone === 'info') {
    return IconTool
  }

  const signal = provider.health.signals.find((item) => `${provider.id}:${item.episode_id}` === attention.episodeKey)

  return (signal && SIGNAL_ICONS[signal.kind]) ?? HEALTH_META[provider.health.level].icon
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
