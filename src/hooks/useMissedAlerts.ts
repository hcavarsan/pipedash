import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { isRemoteClient } from '../services/connection'
import { listenEvent } from '../services/events'
import { mobileService } from '../services/mobile'
import { WS_EVENTS } from '../services/websocket'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'

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
          const { insertMissed } = useNotificationCenterStore.getState()

          for (const alert of alerts) {
            void insertMissed({
              key: alert.key,
              at: Date.parse(alert.created_at),
              tone: alert.tone,
              level: null,
              title: alert.title,
              body: alert.body,
              providerIds: alert.provider_ids,
              pipelineId: alert.pipeline_id,
              link: null,
            })
          }
        })
        .catch((error: unknown) => logger.warn('useMissedAlerts', "Couldn't read the missed alerts", error))
    }

    listenEvent<{ status: string }>(WS_EVENTS.CONNECTION_STATUS, ({ status }) => {
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
