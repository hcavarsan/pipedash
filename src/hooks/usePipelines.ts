import { useCallback, useEffect, useState } from 'react'

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
      } finally {
        setLoading(false)
      }
    },
    [providerId]
  )

  const refresh = useCallback(async () => {
    await loadPipelines(false, true)
  }, [loadPipelines])

  useEffect(() => {
    let unlistenPipelines: (() => void) | null = null
    let unlistenProviders: (() => void) | null = null
    let isMounted = true

    const setup = async () => {
      if (isMounted && initialLoad) {
        await loadPipelines(true, pipelines.length === 0)
        setInitialLoad(false)
      }

      const unlistenPipelinesUpdated = await listen<Pipeline[]>('pipelines-updated', (event) => {
        if (isMounted) {
          setPipelines(event.payload)
        }
      })

      const unlistenProvidersChanged = await listen('providers-changed', async () => {
        if (isMounted) {
          await loadPipelines(true, false)
        }
      })

      unlistenPipelines = unlistenPipelinesUpdated
      unlistenProviders = unlistenProvidersChanged
    }

    setup()

    return () => {
      isMounted = false
      if (unlistenPipelines) {
        unlistenPipelines()
      }
      if (unlistenProviders) {
        unlistenProviders()
      }
    }
  }, [providerId, loadPipelines, initialLoad, pipelines.length])

  return {
    pipelines,
    loading,
    error,
    refresh,
  }
}
