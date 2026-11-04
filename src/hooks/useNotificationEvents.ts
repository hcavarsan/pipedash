import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { isTauri } from '../services/isTauri'
import { events } from '../services/service'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import { useNotificationStore } from '../stores/notificationStore'
import type { ShownNotification } from '../types/domain'
import { isDesktopTauri } from '../utils/platform'
import { showSystemNotification, systemDeliveryEnabled } from '../utils/systemNotifications'

export function useNotificationEvents(): void {
  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined

    events
      .listen<ShownNotification>('notification', (notification) => {
        if (!active) {
          return
        }

        const { key, title, body, pipeline_id, provider_id, provider_ids, link, fired } = notification
        const health = notification.kind === 'outage' || notification.kind === 'recovered'

        if (health && !link && isTauri() && !navigator.onLine) {
          return
        }

        if (fired) {
          useNotificationStore.getState().markFired([fired])
        }

        if (isDesktopTauri()) {
          return
        }

        useNotificationCenterStore.getState().push({
          key,
          at: Date.parse(notification.at),
          tone: notification.tone,
          level: notification.level ?? null,
          title,
          body,
          providerIds: pipeline_id ? [] : provider_ids,
          pipelineId: pipeline_id,
          link: link ?? null,
        })

        if (systemDeliveryEnabled()) {
          showSystemNotification(title, body, {
            key,
            pipelineId: pipeline_id ?? undefined,
            providerId: provider_id ?? provider_ids[0],
          })
        }
      })
      .then((fn) => {
        if (active) {
          unlisten = fn
        } else {
          fn()
        }
      })
      .catch((error: unknown) => logger.warn('useNotificationEvents', "Couldn't listen for notifications", error))

    return () => {
      active = false
      unlisten?.()
    }
  }, [])
}
