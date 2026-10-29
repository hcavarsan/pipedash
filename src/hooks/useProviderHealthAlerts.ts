import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { events, isTauri } from '../services'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import type { EventPayloadMap } from '../types/events'
import { showSystemNotification, systemDeliveryEnabled } from '../utils/systemNotifications'

type HealthAlert = EventPayloadMap['provider-health-alert']

export function useProviderHealthAlerts(): void {
  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined

    events
      .listen<HealthAlert>('provider-health-alert', (alert) => {
        if (!active) {
          return
        }

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

        if (!systemDeliveryEnabled()) {
          return
        }

        showSystemNotification(alert.title, alert.body, {
          key: alert.notification_key,
          providerId: alert.provider_ids[0],
        })
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
