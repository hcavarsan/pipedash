import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { isRemoteClient } from '../services/connection'
import { mobileService } from '../services/mobile'
import { events } from '../services/service'
import { WS_EVENTS } from '../services/websocket'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import { shouldNotify, useNotificationStore } from '../stores/notificationStore'

export function useMissedAlerts(): void {
  useEffect(() => {
    if (!isRemoteClient()) {
      return
    }

    let active = true
    let unlisten: (() => void) | undefined

    const take = () => {
      mobileService
        .takeMissedAlerts()
        .then((alerts) => {
          const { push } = useNotificationCenterStore.getState()

          for (const alert of alerts) {
            const providerIds = alert.provider_ids?.length
              ? alert.provider_ids
              : alert.provider_id === null
                ? []
                : [alert.provider_id]

            if (!shouldNotify(alert.kind, providerIds, alert.pipeline_id, alert.finish ?? null)) {
              continue
            }

            push({
              key: alert.key,
              at: Date.parse(alert.created_at),
              tone: alert.tone,
              level: null,
              title: alert.title,
              body: alert.body,
              providerIds,
              pipelineId: alert.pipeline_id,
              link: null,
            })
          }

          for (const alert of alerts) {
            if (alert.finish && alert.pipeline_id !== null) {
              useNotificationStore.getState().consumeNextFinish(alert.pipeline_id)
            }
          }
        })
        .catch((error: unknown) => logger.warn('useMissedAlerts', "Couldn't read the missed alerts", error))
    }

    events
      .listen<{ status: string }>(WS_EVENTS.CONNECTION_STATUS, ({ status }) => {
        if (status === 'connected') {
          take()
        }
      })
      .then((fn) => {
        if (active) {
          unlisten = fn
        } else {
          fn()
        }
      })
      .catch((error: unknown) => logger.warn('useMissedAlerts', "Couldn't listen for the connection", error))

    take()

    return () => {
      active = false
      unlisten?.()
    }
  }, [])
}
