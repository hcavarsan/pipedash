import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { hasLocalCore } from '../services/connection'
import { tauriService } from '../services/tauri'
import { isAndroidPlatform } from '../utils/platform'

export const useRefresh = (): void => {
  useEffect(() => {
    if (!hasLocalCore()) {
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

    const initiallyFocused = isAndroidPlatform()
      ? Promise.resolve(document.visibilityState === 'visible')
      : appWindow.isFocused()

    initiallyFocused
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
