import { useEffect } from 'react'

import { logger } from '../../../lib/logger'
import { isRemoteClient } from '../../../services/connection'
import { listenEvent } from '../../../services/events'
import { isTauri } from '../../../services/isTauri'
import { mobileService } from '../../../services/mobile'
import { WS_EVENTS } from '../../../services/websocket'
import { useNotificationStore } from '../../../stores/notificationStore'

import { recordFired, refreshPreferenceStates } from './changePreference'

export function useNotificationPreferencesSync(): void {
  useEffect(() => {
    if (!isTauri()) {
      return
    }
    const client = isRemoteClient()
    let active = true
    let unlisten: (() => void) | undefined

    const send = () => {
      mobileService
        .setNotificationPreferences(useNotificationStore.getState().preferences)
        .then(recordFired)
        .catch((error: unknown) =>
          logger.warn('useNotificationPreferencesSync', "Couldn't save the notification preferences", error)
        )
    }

    const sync = async () => {
      if (!client) {
        send()

        return
      }

      try {
        const loaded = await mobileService.loadNotificationPreferences()

        if (!active) {
          return
        }
        if (loaded) {
          useNotificationStore.getState().adoptPreferences(loaded)
          void refreshPreferenceStates()

          return
        }
        send()
      } catch (error) {
        logger.warn('useNotificationPreferencesSync', "Couldn't load the notification preferences", error)
      }
    }

    const resend = () => {
      if (document.visibilityState === 'visible') {
        send()
      }
    }

    if (client) {
      listenEvent<{ status: string }>(WS_EVENTS.CONNECTION_STATUS, ({ status }) => {
        if (status === 'connected') {
          void sync()
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
          logger.warn('useNotificationPreferencesSync', "Couldn't listen for the connection", error)
        )
    }
    void sync()
    document.addEventListener('visibilitychange', resend)
    window.addEventListener('focus', resend)

    return () => {
      active = false
      unlisten?.()
      document.removeEventListener('visibilitychange', resend)
      window.removeEventListener('focus', resend)
    }
  }, [])
}
