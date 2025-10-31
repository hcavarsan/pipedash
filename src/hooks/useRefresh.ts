import { useCallback, useEffect, useState } from 'react'

import { getCurrentWindow } from '@tauri-apps/api/window'

import { tauriService } from '../services/tauri'

export const useRefresh = () => {
  const [mode, setMode] = useState<'active' | 'idle'>('idle')

  const updateMode = useCallback(async (newMode: 'active' | 'idle') => {
    try {
      await tauriService.setRefreshMode(newMode)
      setMode(newMode)
    } catch (err) {
      console.error('Failed to update refresh mode:', err)
    }
  }, [])

  useEffect(() => {
    const loadCurrentMode = async () => {
      try {
        const currentMode = await tauriService.getRefreshMode()


        setMode(currentMode)
      } catch (err) {
        console.error('Failed to load refresh mode:', err)
      }
    }

    loadCurrentMode()

    const appWindow = getCurrentWindow()
    let unlisten: (() => void) | null = null

    const setupListeners = async () => {
      unlisten = await appWindow.onFocusChanged(({ payload: focused }) => {
        updateMode(focused ? 'active' : 'idle')
      })
    }

    setupListeners()

    return () => {
      if (unlisten) {
        unlisten()
      }
    }
  }, [updateMode])

  return {
    mode,
    setMode: updateMode,
  }
}
