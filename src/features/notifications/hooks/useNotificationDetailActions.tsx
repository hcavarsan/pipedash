import { IconBell, IconBellOff, IconExternalLink, IconHeartbeat, IconHistory } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { pushState } from '../../../components/layout/mobileChrome'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useModalStore } from '../../../stores/modalStore'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import type { CenterNotification } from '../../../types/notifications'
import { openNotificationLink } from '../lib/openNotificationLink'
import { type MuteItem, useNotifyItems } from './useNotifyItems'

const LINK_LABELS = { incident: 'View incident', status_page: 'Open status page' } as const

export interface NotificationDetailAction {
  key: string
  label: string
  icon: (size: number) => ReactNode
  chevron: boolean
  onClick: () => void
}

const muteAction = (item: MuteItem, target: string): NotificationDetailAction => {
  const Icon = item.muted ? IconBell : IconBellOff

  return {
    key: 'mute',
    label: `${item.muted ? 'Unmute' : 'Mute'} ${target}`,
    icon: (size) => <Icon size={size} aria-hidden />,
    chevron: false,
    onClick: item.onClick,
  }
}

export function useNotificationDetailActions(notification: CenterNotification | null): NotificationDetailAction[] {
  const open = useNotificationCenterStore((state) => state.open)
  const close = useNotificationCenterStore((state) => state.close)
  const { data: providers } = useProviders()
  const { data: pipelines } = usePipelines()
  const navigate = useNavigate()
  const location = useLocation()
  const { pipelineItem, providerItem, watchItem } = useNotifyItems()

  if (!notification) {
    return []
  }

  const { link, pipelineId, runNumber } = notification
  const related = (providers ?? []).filter((provider) => notification.providerIds.includes(provider.id))
  const [onlyProvider] = related
  const actions: NotificationDetailAction[] = []

  const openHistory = (id: string) => {
    close()
    void navigate(`/pipelines/${encodeURIComponent(id)}`, {
      state: pushState(location.pathname, location.search, location.state),
    })
    if (runNumber) {
      useModalStore.getState().openLogsModal(id, runNumber)
    }
  }

  if (link) {
    actions.push({
      key: 'link',
      label: LINK_LABELS[link.kind],
      icon: (size) => <IconExternalLink size={size} aria-hidden />,
      chevron: true,
      onClick: () => openNotificationLink(link.url),
    })
  }

  if (pipelineId) {
    const name = pipelines?.find((pipeline) => pipeline.id === pipelineId)?.name ?? 'this pipeline'
    const watch = watchItem({ id: pipelineId, name })

    actions.push({
      key: 'history',
      label: runNumber ? 'Open run' : 'Open run history',
      icon: (size) => <IconHistory size={size} aria-hidden />,
      chevron: true,
      onClick: () => openHistory(pipelineId),
    })
    actions.push({ key: 'watch', label: watch.label, icon: () => watch.icon, chevron: false, onClick: watch.onClick })
    actions.push(muteAction(pipelineItem({ id: pipelineId, name }), 'pipeline'))
  } else if (onlyProvider && related.length === 1) {
    actions.push(muteAction(providerItem(onlyProvider), 'provider'))
  }

  for (const provider of related) {
    actions.push({
      key: `provider-${provider.id}`,
      label: related.length > 1 ? `${provider.name} status` : 'View service status',
      icon: (size) => <IconHeartbeat size={size} aria-hidden />,
      chevron: true,
      onClick: () => open({ type: 'provider', providerId: provider.id }),
    })
  }

  return actions
}
