import { useEffect } from 'react'

import { logger } from '../../../lib/logger'
import { isRemoteClient } from '../../../services/connection'
import { isTauri } from '../../../services/isTauri'
import { mobileService } from '../../../services/mobile'
import { events } from '../../../services/service'
import { WS_EVENTS } from '../../../services/websocket'
import { useNotificationStore } from '../../../stores/notificationStore'

const PREFERENCES_DELAY_MS = 300

export function useNotificationPreferencesSync(): void {
  useEffect(() => {
    if (!isTauri()) {
      return
    }
    const client = isRemoteClient()
    let timer: ReturnType<typeof setTimeout> | undefined
    let adopting = false
    let active = true
    let unlisten: (() => void) | undefined

    const send = () => {
      clearTimeout(timer)
      timer = undefined
      mobileService
        .setNotificationPreferences(useNotificationStore.getState().preferences)
        .then((fired) => useNotificationStore.getState().markFired(fired))
        .catch((error: unknown) =>
          logger.warn('useNotificationPreferencesSync', "Couldn't save the notification preferences", error)
        )
    }

    const sync = async () => {
      if (!client || timer !== undefined) {
        send()

        return
      }
      const before = useNotificationStore.getState().preferences

      try {
        const loaded = await mobileService.loadNotificationPreferences()

        if (!active) {
          return
        }
        if (loaded && useNotificationStore.getState().preferences === before) {
          adopting = true
          useNotificationStore.getState().adoptPreferences(loaded)
          adopting = false

          return
        }
        send()
      } catch (error) {
        logger.warn('useNotificationPreferencesSync', "Couldn't load the notification preferences", error)
      }
    }

    const unsubscribe = useNotificationStore.subscribe((state, previous) => {
      if (!adopting && state.preferences !== previous.preferences) {
        clearTimeout(timer)
        timer = setTimeout(send, PREFERENCES_DELAY_MS)
      }
    })
    const resend = () => {
      if (document.visibilityState === 'visible' || timer !== undefined) {
        send()
      }
    }

    if (client) {
      events
        .listen<{ status: string }>(WS_EVENTS.CONNECTION_STATUS, ({ status }) => {
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
      unsubscribe()
      if (timer !== undefined) {
        send()
      }
      document.removeEventListener('visibilitychange', resend)
      window.removeEventListener('focus', resend)
    }
  }, [])
}
