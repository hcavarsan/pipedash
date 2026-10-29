import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { isTauri, tauriService } from '../services'

export const useRefresh = (): void => {
  useEffect(() => {
    if (!isTauri()) {
      return
    }

    let mounted = true
    let focusEventSeen = false
    let unlisten: (() => void) | undefined
    const appWindow = getCurrentWindow()

    const apply = (focused: boolean) => {
      tauriService.setRefreshMode(focused ? 'active' : 'idle').catch((err) => {
        logger.error('useRefresh', 'Failed to update refresh mode', err)
      })
    }

    appWindow
      .isFocused()
      .then((focused) => {
        if (mounted && !focusEventSeen) {
          apply(focused)
        }
      })
      .catch((err) => logger.error('useRefresh', 'Failed to read window focus', err))

    appWindow
      .onFocusChanged(({ payload: focused }) => {
        focusEventSeen = true
        if (mounted) {
          apply(focused)
        }
      })
      .then((stop) => {
        if (mounted) {
          unlisten = stop
        } else {
          stop()
        }
      })
      .catch((err) => logger.error('useRefresh', 'Failed to setup Tauri window listener', err))

    return () => {
      mounted = false
      unlisten?.()
    }
  }, [])
}
