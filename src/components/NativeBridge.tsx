import { listen } from '@tauri-apps/api/event'
import { type ReactNode, useEffect } from 'react'

import { logger } from '../lib/logger'
import { isTauri } from '../services/isTauri'
import { mobileService, STATUS_BAR_TAP_EVENT } from '../services/mobile'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import { isMobilePlatform } from '../utils/platform'

import { scrollActiveToTop } from './layout/scrollToTop'

function listenNative(event: string, onEvent: () => void): () => void {
  let stop: (() => void) | undefined
  let active = true

  listen(event, onEvent)
    .then((unlisten) => {
      if (active) {
        stop = unlisten
      } else {
        unlisten()
      }
    })
    .catch((error: unknown) => logger.warn('NativeBridge', `Couldn't listen for ${event}`, error))

  return () => {
    active = false
    stop?.()
  }
}

export function NativeBridge(): ReactNode {
  useEffect(() => {
    if (!isTauri()) {
      return
    }

    const stopTaps = listenNative(STATUS_BAR_TAP_EVENT, scrollActiveToTop)

    return () => {
      stopTaps()
    }
  }, [])

  useEffect(() => {
    if (!isMobilePlatform()) {
      return
    }

    const unread = (items: { read: boolean }[]) => items.filter((item) => !item.read).length
    let last = unread(useNotificationCenterStore.getState().items)

    mobileService.setBadge(last).catch(() => undefined)

    return useNotificationCenterStore.subscribe((state) => {
      const count = unread(state.items)
      if (count !== last) {
        last = count
        mobileService.setBadge(count).catch(() => undefined)
      }
    })
  }, [])

  return null
}
