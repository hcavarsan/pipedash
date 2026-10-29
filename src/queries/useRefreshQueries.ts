import { notifications } from '@mantine/notifications'
import { type UseMutationResult, useMutation, useMutationState, useQueryClient } from '@tanstack/react-query'

import { queryKeys } from '../lib/queryKeys'
import { isTauri, service } from '../services'
import type { ProviderSummary, RefreshResult } from '../types'
import { displayErrorNotification } from '../utils/errorDisplay'
import { summarizeRefresh } from '../utils/refreshSummary'

const REFRESH_TOAST_MS = 5000

const desktopOffline = () => isTauri() && !navigator.onLine

const REFRESH_PROVIDER_KEY = ['providers', 'refresh'] as const

export function useRefreshProvider(): UseMutationResult<RefreshResult, Error, number, unknown> {
  return useMutation({
    mutationKey: REFRESH_PROVIDER_KEY,
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
  const pendingIds = useMutationState({
    filters: { mutationKey: REFRESH_PROVIDER_KEY, status: 'pending' },
    select: (mutation) => mutation.state.variables,
  })
  const refreshing: ReadonlySet<number> = new Set(pendingIds.filter((id): id is number => typeof id === 'number'))

  const refresh = async (providerIds: number[]): Promise<void> => {
    const ids = providerIds.filter((id) => !refreshing.has(id))

    if (ids.length === 0) {
      return
    }

    const settled = await Promise.allSettled(ids.map((id) => refreshProvider(id)))

    if (!desktopOffline()) {
      const results = settled.flatMap((entry) => (entry.status === 'fulfilled' ? [entry.value] : []))
      const failed = settled.find((entry): entry is PromiseRejectedResult => entry.status === 'rejected')

      if (results.length > 0) {
        const providers = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? []
        const { message, color } = summarizeRefresh(results, providers)

        notifications.show({ message, color, autoClose: REFRESH_TOAST_MS })
      }

      if (failed) {
        displayErrorNotification(failed.reason, "Couldn't refresh")
      }
    }

    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
    ])
  }

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
