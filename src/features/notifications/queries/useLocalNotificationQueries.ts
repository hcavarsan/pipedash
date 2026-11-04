import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect } from 'react'

import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import { hasLocalCore, isRemoteClient } from '../../../services/connection'
import { mobileService } from '../../../services/mobile'
import { events } from '../../../services/service'
import { WS_EVENTS } from '../../../services/websocket'
import { useNotificationStore } from '../../../stores/notificationStore'
import type { LocalNotificationSettings } from '../../../types/domain'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'
import { isMobilePlatform } from '../../../utils/platform'

const mutationKey = [...queryKeys.localNotifications.all, 'change']
const PREFERENCES_DELAY_MS = 300

export function useLocalNotificationSettings(): UseQueryResult<LocalNotificationSettings, Error> {
  return useQuery({
    queryKey: queryKeys.localNotifications.settings(),
    queryFn: () => mobileService.getLocalNotificationSettings(),
    enabled: isMobilePlatform() && (hasLocalCore() || isRemoteClient()),
    staleTime: 0,
    retry: false,
  })
}

export function useSetLocalNotifications(): UseMutationResult<LocalNotificationSettings, Error, boolean, void> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey,
    mutationFn: (enabled: boolean) => mobileService.setLocalNotificationsEnabled(enabled),
    onMutate: () => queryClient.cancelQueries({ queryKey: queryKeys.localNotifications.settings() }),
    onSuccess: (settings) => {
      queryClient.setQueryData(queryKeys.localNotifications.settings(), settings)
    },
    onError: (error, enabled) =>
      displayErrorNotification(error, enabled ? "Couldn't turn on notifications" : "Couldn't turn off notifications"),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() }),
  })
}

export function useRemoveAlertHost(): UseMutationResult<void, Error, void> {
  const queryClient = useQueryClient()

  return useMutation<void, Error, void>({
    mutationFn: () => mobileService.removeAlertHost(),
    onError: (error) => displayErrorNotification(error, "Couldn't remove the server"),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() }),
  })
}

export function useSendTestAlert(): UseMutationResult<void, Error, void> {
  return useMutation<void, Error, void>({
    mutationFn: () => mobileService.sendTestAlert(),
    onSuccess: () => displaySuccessNotification('Check your notifications in a moment.', 'Test alert sent'),
    onError: (error) => displayErrorNotification(error, "Couldn't send test alert"),
  })
}

export function useLocalNotificationsLifecycle(): void {
  const queryClient = useQueryClient()
  useEffect(() => {
    if (!(isMobilePlatform() && (hasLocalCore() || isRemoteClient()))) {
      return
    }
    const refresh = () => {
      if (document.visibilityState === 'hidden' || queryClient.isMutating({ mutationKey }) > 0) {
        return
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() })
    }
    let timer: ReturnType<typeof setTimeout> | undefined
    const send = () => {
      clearTimeout(timer)
      timer = undefined
      const sent = useNotificationStore.getState().preferences
      mobileService
        .setNotificationPreferences(sent)
        .then((fired) => {
          const { preferences, cancelNextFinish } = useNotificationStore.getState()
          for (const pipelineId of fired) {
            const used = sent.watch_once.find((watch) => watch.pipeline_id === pipelineId)
            const pending = preferences.watch_once.find((watch) => watch.pipeline_id === pipelineId)
            if (used && pending?.since === used.since) {
              cancelNextFinish(pipelineId)
            }
          }
        })
        .catch((error: unknown) =>
          logger.warn('useLocalNotificationsLifecycle', "Couldn't save the notification preferences", error)
        )
    }
    const unsubscribe = useNotificationStore.subscribe((state, previous) => {
      if (state.preferences !== previous.preferences) {
        clearTimeout(timer)
        timer = setTimeout(send, PREFERENCES_DELAY_MS)
      }
    })
    const sync = () => {
      if (document.visibilityState === 'visible' || timer !== undefined) {
        send()
      }
    }
    let unlisten: (() => void) | undefined
    let active = true
    events
      .listen<{ status: string }>(WS_EVENTS.CONNECTION_STATUS, ({ status }) => {
        if (status === 'connected') {
          send()
        }
      })
      .then((fn) => {
        if (active) {
          unlisten = fn
        } else {
          fn()
        }
      })
      .catch((error: unknown) =>
        logger.warn('useLocalNotificationsLifecycle', "Couldn't listen for the connection", error)
      )
    send()
    document.addEventListener('visibilitychange', refresh)
    document.addEventListener('visibilitychange', sync)
    window.addEventListener('focus', refresh)
    window.addEventListener('focus', sync)

    return () => {
      active = false
      unlisten?.()
      unsubscribe()
      if (timer !== undefined) {
        send()
      }
      document.removeEventListener('visibilitychange', refresh)
      document.removeEventListener('visibilitychange', sync)
      window.removeEventListener('focus', refresh)
      window.removeEventListener('focus', sync)
    }
  }, [queryClient])
}
