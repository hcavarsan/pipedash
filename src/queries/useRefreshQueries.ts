import { notifications } from '@mantine/notifications'
import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query'
import { useCallback, useState } from 'react'

import { queryKeys } from '../lib/queryKeys'
import { isTauri, service } from '../services'
import type { ProviderSummary, RefreshResult } from '../types'
import { displayErrorNotification } from '../utils/errorDisplay'
import { summarizeRefresh } from '../utils/refreshSummary'

const REFRESH_TOAST_MS = 5000

const desktopOffline = () => isTauri() && !navigator.onLine

export function useRefreshProvider(): UseMutationResult<RefreshResult, Error, number, unknown> {
  return useMutation({
    mutationFn: (providerId: number) => service.refreshProvider(providerId),
  })
}

interface RefreshProvidersResult {
  refresh: (providerIds: number[]) => Promise<void>
  refreshing: ReadonlySet<number>
}

export function useRefreshProviders(): RefreshProvidersResult {
  const queryClient = useQueryClient()
  const { mutateAsync: refreshProvider } = useRefreshProvider()
  const [refreshing, setRefreshing] = useState<ReadonlySet<number>>(new Set())

  const refresh = useCallback(
    async (providerIds: number[]) => {
      const ids = providerIds.filter((id) => !refreshing.has(id))

      if (ids.length === 0) {
        return
      }

      setRefreshing((current) => new Set([...current, ...ids]))

      try {
        const results = await Promise.all(ids.map((id) => refreshProvider(id)))

        if (!desktopOffline()) {
          const providers = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? []
          const { message, color } = summarizeRefresh(results, providers)

          notifications.show({ message, color, autoClose: REFRESH_TOAST_MS })
        }
      } catch (error) {
        if (!desktopOffline()) {
          displayErrorNotification(error, "Couldn't refresh")
        }
      } finally {
        setRefreshing((current) => new Set([...current].filter((id) => !ids.includes(id))))
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
          queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
        ])
      }
    },
    [queryClient, refreshProvider, refreshing]
  )

  return { refresh, refreshing }
}

export function useRefreshAll(): UseMutationResult<RefreshResult[], Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.refreshAll(),

    onSuccess: (results) => {
      if (desktopOffline()) {
        return
      }

      const providers = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? []
      const { message, color } = summarizeRefresh(results, providers)

      notifications.show({ message, color, autoClose: REFRESH_TOAST_MS })
    },

    onError: (error) => {
      if (!desktopOffline()) {
        displayErrorNotification(error, "Couldn't refresh")
      }
    },

    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
        queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
      ]),
  })
}
