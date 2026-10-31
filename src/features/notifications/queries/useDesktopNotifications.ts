import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

import { logger } from '../../../lib/logger'
import {
  DESKTOP_NOTIFICATIONS_EVENT,
  type DesktopNotificationSnapshot,
  desktopService,
} from '../../../services/desktop'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import { isDesktopTauri } from '../../../utils/platform'

export function useDesktopNotifications(): void {
  useEffect(() => {
    if (!isDesktopTauri()) {
      return
    }
    let active = true
    let remove: (() => void) | undefined
    const apply = (snapshot: DesktopNotificationSnapshot) => {
      if (active) {
        useNotificationCenterStore.getState().applyDesktopSnapshot(snapshot)
      }
    }

    listen<DesktopNotificationSnapshot>(DESKTOP_NOTIFICATIONS_EVENT, ({ payload }) => apply(payload))
      .then(async (unlisten) => {
        if (!active) {
          unlisten()
          return
        }
        remove = unlisten
        apply(await desktopService.getNotifications())
      })
      .catch((error: unknown) => logger.warn('Desktop', "Couldn't load desktop notifications", error))

    return () => {
      active = false
      remove?.()
    }
  }, [])
}
