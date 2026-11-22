import { IconBell, IconBellOff, IconBellPlus, IconBellRinging } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useNotificationStore } from '../../../stores/notificationStore'
import type { PipelineNotifyState, PreferenceChange } from '../../../types/notification_preferences'
import type { Pipeline } from '../../../types/pipelines'
import { displaySuccessNotification } from '../../../utils/errorDisplay'
import { formatGap } from '../../../utils/pipelinePresentation'
import { changePreference } from '../queries/changePreference'

export interface NotifyItem {
  label: string
  icon: ReactNode
  onClick: () => void
}

export interface MuteItem extends NotifyItem {
  muted: boolean
}

export interface PolicyOption {
  value: PipelineNotifyState
  label: string
  description: string
  icon: ReactNode
  selected: boolean
  select: () => Promise<boolean>
}

export type PolicyTarget = Pick<Pipeline, 'id' | 'name' | 'status' | 'cadence'>

export interface QuietItem {
  label: string
  description: string
  enabled: boolean
  available: boolean
  toggle: () => Promise<boolean>
}

interface NotifyItems {
  pipelineState: (pipelineId: string) => PipelineNotifyState
  policyOptions: (pipeline: PolicyTarget) => PolicyOption[]
  quietItem: (pipeline: PolicyTarget) => QuietItem

  providerItem: (provider: { id: number; name: string }) => MuteItem
}

const UNMUTE_HINT = 'Unmute it from its menu or in notification settings.'

const POLICY_ICONS: Record<PipelineNotifyState, ReactNode> = {
  default: <IconBell size={16} aria-hidden />,
  pending: <IconBellPlus size={16} aria-hidden />,
  watching: <IconBellRinging size={16} aria-hidden />,
  muted: <IconBellOff size={16} aria-hidden />,
}

const leave = (state: PipelineNotifyState, id: string): PreferenceChange | null => {
  switch (state) {
    case 'watching':
      return { type: 'pipeline_watched', pipeline_id: id, watched: false }
    case 'pending':
      return { type: 'cancel_next_finish', pipeline_id: id }
    case 'muted':
      return { type: 'pipeline_muted', pipeline_id: id, muted: false }
    case 'default':
      return null
  }
}

const enter = (state: Exclude<PipelineNotifyState, 'default'>, id: string): PreferenceChange => {
  switch (state) {
    case 'watching':
      return { type: 'pipeline_watched', pipeline_id: id, watched: true }
    case 'pending':
      return { type: 'watch_next_finish', pipeline_id: id }
    case 'muted':
      return { type: 'pipeline_muted', pipeline_id: id, muted: true }
  }
}

const confirmation = (state: PipelineNotifyState, name: string): [message: string, title: string] => {
  switch (state) {
    case 'watching':
      return [`You'll be notified every time ${name} finishes.`, 'Watching pipeline']
    case 'pending':
      return ['Change it from this menu or in notification settings.', `You'll be notified when ${name} finishes`]
    case 'muted':
      return [`You won't get notifications about ${name}. ${UNMUTE_HINT}`, 'Pipeline muted']
    case 'default':
      return [`${name} follows your notification settings again.`, 'Following notification settings']
  }
}

export function useNotifyItems(): NotifyItems {
  const preferences = useNotificationStore((state) => state.preferences)
  const pipelineStates = useNotificationStore((state) => state.pipelineStates)

  const pipelineState = (id: string): PipelineNotifyState => pipelineStates[id] ?? 'default'

  const setPolicy = async ({ id, name }: PolicyTarget, target: PipelineNotifyState): Promise<boolean> => {
    const current = pipelineState(id)
    const change = target === 'default' ? leave(current, id) : enter(target, id)

    if (current === target || !change) {
      return true
    }

    if (!(await changePreference(change))) {
      return false
    }

    displaySuccessNotification(...confirmation(target, name))

    return true
  }

  const policyOptions: NotifyItems['policyOptions'] = (pipeline) => {
    const current = pipelineState(pipeline.id)
    const active = pipeline.status === 'running' || pipeline.status === 'pending'
    const copy: Record<PipelineNotifyState, [label: string, description: string]> = {
      default: ['Follow notification settings', 'Uses the kinds and providers chosen in Settings.'],
      pending: [
        active ? 'Notify when this run finishes' : 'Notify when the next run finishes',
        'One notification, then back to your settings.',
      ],
      watching: ['Watch all runs', 'Notifies every time it finishes, even if its provider is muted.'],
      muted: ['Mute notifications', 'No notifications about this pipeline.'],
    }

    return (['default', 'pending', 'watching', 'muted'] as const).map((value) => ({
      value,
      label: copy[value][0],
      description: copy[value][1],
      icon: POLICY_ICONS[value],
      selected: current === value,
      select: () => setPolicy(pipeline, value),
    }))
  }

  const quietItem: NotifyItems['quietItem'] = ({ id, name, cadence }) => {
    const enabled = preferences.quiet_pipelines.includes(id)
    const description = cadence
      ? `One notification when no run shows up for about twice its usual gap of ${formatGap(cadence.usual_gap_seconds)}.`
      : 'Needs a few more runs to learn how often it runs.'

    return {
      label: 'Tell me if it stops running',
      description,
      enabled,
      available: enabled || Boolean(cadence),
      toggle: async () => {
        if (!(await changePreference({ type: 'quiet_alert', pipeline_id: id, enabled: !enabled }))) {
          return false
        }

        displaySuccessNotification(
          enabled
            ? `You won't be told when ${name} misses its usual schedule.`
            : `You'll be notified if ${name} stops running on its usual schedule.`,
          enabled ? 'Missed runs ignored' : 'Watching for missed runs'
        )

        return true
      },
    }
  }

  return {
    pipelineState,
    policyOptions,
    quietItem,
    providerItem: ({ id, name }) => {
      const muted = preferences.muted_providers.includes(id)

      return {
        label: muted ? 'Unmute notifications' : 'Mute notifications',
        icon: muted ? <IconBell size={16} aria-hidden /> : <IconBellOff size={16} aria-hidden />,
        muted,
        onClick: async () => {
          if (!(await changePreference({ type: 'provider_muted', provider_id: id, muted: !muted }))) {
            return
          }

          displaySuccessNotification(
            muted
              ? `${name} is no longer muted.`
              : `You won't get notifications from ${name}, except for pipelines you watch. ${UNMUTE_HINT}`,
            `Provider ${muted ? 'unmuted' : 'muted'}`
          )
        },
      }
    },
  }
}
