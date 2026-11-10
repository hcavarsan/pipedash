import { IconBell, IconBellOff, IconExternalLink, IconHeartbeat, IconHistory } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { pushState } from '../../../components/layout/mobileChrome'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useModalStore } from '../../../stores/modalStore'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import type { CenterNotification } from '../../../types/notifications'
import { openLink } from '../../../utils/openExternal'
import { usePipelineNotifyItem } from '../components/PipelineNotifyPolicy'
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
  const { providerItem } = useNotifyItems()
  const notifyItem = usePipelineNotifyItem()

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
      onClick: () => openLink(link.url),
    })
  }

  if (pipelineId) {
    const target = pipelines?.find((pipeline) => pipeline.id === pipelineId)

    actions.push({
      key: 'history',
      label: runNumber ? 'Open run' : 'Open run history',
      icon: (size) => <IconHistory size={size} aria-hidden />,
      chevron: true,
      onClick: () => openHistory(pipelineId),
    })

    if (target) {
      const notify = notifyItem(target)

      actions.push({
        key: 'notify',
        label: notify.label,
        icon: (size) => <IconBell size={size} aria-hidden />,
        chevron: true,
        onClick: notify.onClick,
      })
    }
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
