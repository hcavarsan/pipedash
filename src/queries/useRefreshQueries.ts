import { type UseMutationResult, useMutation, useMutationState, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '../lib/queryKeys'
import { isTauri } from '../services/isTauri'
import { service } from '../services/service'
import type { RefreshResult } from '../types/health'
import type { ProviderSummary } from '../types/providers'
import {
  displayErrorNotification,
  displaySuccessNotification,
  displayWarningNotification,
  errorMessage,
} from '../utils/errorDisplay'
import { summarizeRefresh } from '../utils/refreshSummary'

function notifyRefresh(results: RefreshResult[], providers: ProviderSummary[]): void {
  const { message, color } = summarizeRefresh(results, providers)

  if (color === 'green') {
    displaySuccessNotification(message, 'Refreshed')
  } else if (results.every((result) => result.state === 'failed')) {
    displayErrorNotification(new Error(message), "Couldn't refresh providers")
  } else {
    displayWarningNotification(message, 'Refreshed with errors')
  }
}

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

    const results = await Promise.all(
      ids.map((id) =>
        refreshProvider(id).catch(
          (error: unknown): RefreshResult => ({
            provider_id: id,
            state: 'failed',
            kind: null,
            message: errorMessage(error, 'Refresh failed'),
          })
        )
      )
    )

    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
    ])

    if (!desktopOffline()) {
      notifyRefresh(results, queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? [])
    }
  }

  return { refresh, refreshing }
}

export function useRefreshAll(): UseMutationResult<RefreshResult[], Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.refreshAll(),

    onSuccess: async (results) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
        queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
      ])

      if (!desktopOffline()) {
        notifyRefresh(results, queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? [])
      }
    },

    onError: async (error) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
        queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
      ])

      if (!desktopOffline()) {
        displayErrorNotification(error, "Couldn't refresh providers")
      }
    },
  })
}
