import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { isRemoteClient } from '../services/connection'
import { mobileService } from '../services/mobile'
import { events } from '../services/service'
import { WS_EVENTS } from '../services/websocket'
import { type CenterNotificationTone, useNotificationCenterStore } from '../stores/notificationCenterStore'
import type { HostAlert } from '../types/domain'

const TONES: Record<HostAlert['kind'], CenterNotificationTone> = {
  failed: 'danger',
  degraded: 'danger',
  long_run: 'warning',
  outage: 'warning',
  recovered: 'success',
  test: 'success',
}

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
            push({
              key: alert.key,
              at: Date.parse(alert.created_at),
              tone: TONES[alert.kind],
              level: null,
              title: alert.title,
              body: alert.body,
              providerIds: alert.provider_id === null ? [] : [alert.provider_id],
              pipelineId: alert.pipeline_id,
              link: null,
            })
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
