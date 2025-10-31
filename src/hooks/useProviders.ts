import { useCallback, useEffect, useState } from 'react'

import { notifications } from '@mantine/notifications'

import { tauriService } from '../services/tauri'
import type { ProviderConfig, ProviderSummary } from '../types'

export const useProviders = () => {
  const [providers, setProviders] = useState<ProviderSummary[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadProviders = useCallback(async () => {
    try {
      setLoading(true)
      setError(null)
      const data = await tauriService.listProviders()


      setProviders(data)
    } catch (err: any) {
      const errorMsg = err?.error || err?.message || 'Failed to load providers'


      setError(errorMsg)
      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }, [])

  const addProvider = useCallback(
    async (config: ProviderConfig) => {
      try {
        setLoading(true)
        const providerId = await tauriService.addProvider(config)


        await tauriService.fetchPipelines(providerId)
        await loadProviders()
        notifications.show({
          title: 'Success',
          message: `Provider "${config.name}" added successfully`,
          color: 'green',
        })
      } catch (err: any) {
        const errorMsg = err?.error || err?.message || 'Failed to add provider'


        notifications.show({
          title: 'Error',
          message: errorMsg,
          color: 'red',
        })
        throw err
      } finally {
        setLoading(false)
      }
    },
    [loadProviders]
  )

  const updateProvider = useCallback(
    async (id: number, config: ProviderConfig) => {
      try {
        setLoading(true)
        await tauriService.updateProvider(id, config)
        await tauriService.fetchPipelines(id)
        await loadProviders()
        notifications.show({
          title: 'Success',
          message: `Provider "${config.name}" updated successfully`,
          color: 'green',
        })
      } catch (err: any) {
        const errorMsg = err?.error || err?.message || 'Failed to update provider'


        notifications.show({
          title: 'Error',
          message: errorMsg,
          color: 'red',
        })
        throw err
      } finally {
        setLoading(false)
      }
    },
    [loadProviders]
  )

  const removeProvider = useCallback(
    async (id: number, name: string) => {
      try {
        setLoading(true)
        await tauriService.removeProvider(id)
        await loadProviders()
        notifications.show({
          title: 'Success',
          message: `Provider "${name}" removed successfully`,
          color: 'green',
        })
      } catch (err: any) {
        const errorMsg = err?.error || err?.message || 'Failed to remove provider'


        notifications.show({
          title: 'Error',
          message: errorMsg,
          color: 'red',
        })
        throw err
      } finally {
        setLoading(false)
      }
    },
    [loadProviders]
  )

  useEffect(() => {
    loadProviders()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return {
    providers,
    loading,
    error,
    addProvider,
    updateProvider,
    removeProvider,
    refresh: loadProviders,
  }
}
