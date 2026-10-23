import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { events, isTauri } from '../services'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import { useNotificationStore } from '../stores/notificationStore'
import type { EventPayloadMap } from '../types/events'
import { showSystemNotification } from '../utils/systemNotifications'

type HealthAlert = EventPayloadMap['provider-health-alert']

export function useProviderHealthAlerts() {
  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined

    events
      .listen<HealthAlert>('provider-health-alert', (alert) => {
        if (isTauri() && !navigator.onLine && alert.link === null) {
          return
        }

        useNotificationCenterStore.getState().push({
          key: alert.notification_key,
          at: Date.now(),
          tone: alert.recovered ? 'success' : 'warning',
          level: alert.level,
          title: alert.title,
          body: alert.body,
          providerIds: alert.provider_ids,
          pipelineId: null,
          link: alert.link,
        })

        if (!useNotificationStore.getState().providerAlertsEnabled) {
          return
        }

        showSystemNotification(alert.title, alert.body, alert.notification_key)
      })
      .then((fn) => {
        if (active) {
          unlisten = fn
        } else {
          fn()
        }
      })
      .catch((error: unknown) => logger.warn('useProviderHealthAlerts', "Couldn't listen for health alerts", error))

    return () => {
      active = false
      unlisten?.()
    }
  }, [])
}
