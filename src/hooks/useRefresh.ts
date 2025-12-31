import { useCallback, useEffect, useState } from 'react'

import { logger } from '../lib/logger'
import { isTauri, service } from '../services'

export const useRefresh = () => {
  const [mode, setMode] = useState<'active' | 'idle'>('idle')

  const updateMode = useCallback(async (newMode: 'active' | 'idle') => {
    try {
      await service.setRefreshMode(newMode)
      setMode(newMode)
    } catch (err) {
      logger.error('useRefresh', 'Failed to update refresh mode', err)
    }
  }, [])

  useEffect(() => {
    let mounted = true
    let unlistenFn: (() => void) | null = null

    const loadCurrentMode = async () => {
      try {
        const currentMode = await service.getRefreshMode()

        if (mounted) {
          setMode(currentMode)
        }
      } catch (err) {
        logger.error('useRefresh', 'Failed to load refresh mode', err)
      }
    }

    const setupListeners = async () => {
      if (isTauri()) {
        try {
          const { getCurrentWindow } = await import('@tauri-apps/api/window')
          const appWindow = getCurrentWindow()

          const tauriUnlisten = await appWindow.onFocusChanged(({ payload: focused }) => {
            if (mounted) {
              updateMode(focused ? 'active' : 'idle')
            }
          })

          if (!mounted) {
            try {
              tauriUnlisten()
            } catch (e) {
              logger.debug('useRefresh', 'Error cleaning up listener during unmount', e)
            }
            
return
          }
          unlistenFn = tauriUnlisten
        } catch (err) {
          logger.error('useRefresh', 'Failed to setup Tauri window listener', err)
        }
      } else {
        const handleVisibilityChange = () => {
          if (mounted) {
            updateMode(document.visibilityState === 'visible' ? 'active' : 'idle')
          }
        }

        document.addEventListener('visibilitychange', handleVisibilityChange)
        unlistenFn = () => {
          document.removeEventListener('visibilitychange', handleVisibilityChange)
        }

        if (document.visibilityState === 'visible') {
          updateMode('active')
        }
      }
    }

    loadCurrentMode()
    setupListeners()

    return () => {
      mounted = false
      if (unlistenFn) {
        try {
          unlistenFn()
        } catch (e) {
          logger.debug('useRefresh', 'Error during listener cleanup (safe to ignore)', e)
        }
      }
    }
  }, [updateMode])

  return {
    mode,
    setMode: updateMode,
  }
}
