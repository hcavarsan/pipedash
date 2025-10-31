import { useCallback, useEffect, useState } from 'react'

import { notifications } from '@mantine/notifications'
import { listen } from '@tauri-apps/api/event'

import { tauriService } from '../services/tauri'
import type { Pipeline } from '../types'

export const usePipelines = (providerId?: number) => {
  const [pipelines, setPipelines] = useState<Pipeline[]>([])
  const [loading, setLoading] = useState(false)
  const [initialLoad, setInitialLoad] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const loadPipelines = useCallback(
    async (useCache = false, showLoading = false) => {
      try {
        if (showLoading) {
          setLoading(true)
        }
        setError(null)
        const data = useCache
          ? await tauriService.getCachedPipelines(providerId)
          : await tauriService.fetchPipelines(providerId)


        setPipelines(data)
      } catch (err: any) {
        const errorMsg = err?.error || err?.message || 'Failed to load pipelines'


        setError(errorMsg)
        if (!showLoading) {
          notifications.show({
            title: 'Error',
            message: errorMsg,
            color: 'red',
          })
        }
      } finally {
        setLoading(false)
      }
    },
    [providerId]
  )

  const refresh = useCallback(async () => {
    await loadPipelines(false, false)
  }, [loadPipelines])

  useEffect(() => {
    let unlistenFn: (() => void) | null = null
    let isMounted = true

    const setup = async () => {
      if (isMounted && initialLoad) {
        await loadPipelines(true, pipelines.length === 0)
        setInitialLoad(false)
      }

      // Set up event listener
      const unlisten = await listen<Pipeline[]>('pipelines-updated', (event) => {
        if (isMounted) {
          setPipelines(event.payload)
        }
      })


      unlistenFn = unlisten
    }

    setup()

    return () => {
      isMounted = false
      if (unlistenFn) {
        unlistenFn()
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [providerId])

  return {
    pipelines,
    loading,
    error,
    refresh,
  }
}
