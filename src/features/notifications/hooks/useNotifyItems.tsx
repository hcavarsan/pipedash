import { IconBell, IconBellOff, IconBellPlus, IconBellRinging, IconBellX } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useNotificationStore } from '../../../stores/notificationStore'
import type { PipelineNotifyState } from '../../../types/notification_preferences'
import type { Pipeline } from '../../../types/pipelines'
import { displaySuccessNotification } from '../../../utils/errorDisplay'
import { changePreference } from '../queries/changePreference'

export interface NotifyItem {
  label: string
  icon: ReactNode
  onClick: () => void
}

export interface MuteItem extends NotifyItem {
  muted: boolean
}

type PipelineTarget = Pick<Pipeline, 'id' | 'name'>

interface NotifyItems {
  pipelineState: (pipelineId: string) => PipelineNotifyState
  pipelineItem: (pipeline: PipelineTarget) => MuteItem
  watchItem: (pipeline: PipelineTarget) => NotifyItem
  finishItem: (pipeline: PipelineTarget & Pick<Pipeline, 'status'>) => NotifyItem
  notifyItems: (pipeline: PipelineTarget & Pick<Pipeline, 'status'>) => NotifyItem[]
  providerItem: (provider: { id: number; name: string }) => MuteItem
}

const UNMUTE_HINT = 'Unmute it from its menu or in notification settings.'

const muteItem = (
  target: 'Pipeline' | 'Provider',
  name: string,
  muted: boolean,
  toggle: () => void,
  mutedMessage: string
): MuteItem => ({
  label: muted ? 'Unmute notifications' : 'Mute notifications',
  icon: muted ? <IconBell size={16} aria-hidden /> : <IconBellOff size={16} aria-hidden />,
  muted,
  onClick: () => {
    toggle()
    displaySuccessNotification(
      muted ? `${name} is no longer muted.` : mutedMessage,
      `${target} ${muted ? 'unmuted' : 'muted'}`
    )
  },
})

const finishLabel = (pending: boolean, status: Pipeline['status']): string => {
  if (pending) {
    return 'Cancel finish notification'
  }
  if (status === 'running' || status === 'pending') {
    return 'Notify when this run finishes'
  }
  return 'Notify when the next run finishes'
}

export function useNotifyItems(): NotifyItems {
  const preferences = useNotificationStore((state) => state.preferences)
  const pipelineStates = useNotificationStore((state) => state.pipelineStates)

  const pipelineState = (id: string): PipelineNotifyState => pipelineStates[id] ?? 'default'

  const watchItem: NotifyItems['watchItem'] = ({ id, name }) => {
    const watching = pipelineState(id) === 'watching'

    return {
      label: watching ? 'Stop watching' : 'Watch all runs',
      icon: watching ? <IconBellX size={16} aria-hidden /> : <IconBellRinging size={16} aria-hidden />,
      onClick: () => {
        void changePreference({ type: 'pipeline_watched', pipeline_id: id, watched: !watching })
        if (watching) {
          displaySuccessNotification(`${name} follows your notification settings again.`, 'Stopped watching')
        } else {
          displaySuccessNotification(`You'll be notified every time ${name} finishes.`, 'Watching pipeline')
        }
      },
    }
  }

  const finishItem: NotifyItems['finishItem'] = ({ id, name, status }) => {
    const pending = pipelineState(id) === 'pending'

    return {
      label: finishLabel(pending, status),
      icon: pending ? <IconBellX size={16} aria-hidden /> : <IconBellPlus size={16} aria-hidden />,
      onClick: () => {
        if (pending) {
          void changePreference({ type: 'cancel_next_finish', pipeline_id: id })
          displaySuccessNotification(
            `${name} no longer has a pending finish notification.`,
            'Finish notification cancelled'
          )
        } else {
          void changePreference({ type: 'watch_next_finish', pipeline_id: id })
          displaySuccessNotification(
            'Cancel it from this menu or in notification settings.',
            `You'll be notified when ${name} finishes`
          )
        }
      },
    }
  }

  const pipelineItem: NotifyItems['pipelineItem'] = ({ id, name }) => {
    const muted = pipelineState(id) === 'muted'

    return muteItem(
      'Pipeline',
      name,
      muted,
      () => void changePreference({ type: 'pipeline_muted', pipeline_id: id, muted: !muted }),
      `You won't get notifications about ${name}. ${UNMUTE_HINT}`
    )
  }

  return {
    pipelineState,
    pipelineItem,
    watchItem,
    finishItem,
    notifyItems: (pipeline) => [
      watchItem(pipeline),
      ...(pipelineState(pipeline.id) === 'watching' ? [] : [finishItem(pipeline)]),
      pipelineItem(pipeline),
    ],
    providerItem: ({ id, name }) => {
      const muted = preferences.muted_providers.includes(id)

      return muteItem(
        'Provider',
        name,
        muted,
        () => void changePreference({ type: 'provider_muted', provider_id: id, muted: !muted }),
        `You won't get notifications from ${name}, except for pipelines you watch. ${UNMUTE_HINT}`
      )
    },
  }
}
