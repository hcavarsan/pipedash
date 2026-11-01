import {
  type QueryClient,
  skipToken,
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect, useReducer } from 'react'

import { queryKeys } from '../../../lib/queryKeys'
import { DESKTOP_UPDATE_RECONNECT_EVENT, DESKTOP_UPDATE_RESET_EVENT, desktopService } from '../../../services/desktop'
import type { UpdateStatus } from '../../../types/updater'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isRecord } from '../../../utils/isRecord'
import { isDesktopTauri } from '../../../utils/platform'

export type UpdateConnection = 'connecting' | 'connected' | 'failed'

export function latestUpdateStatus(previous: unknown, next: unknown): unknown {
  return isRecord(previous) &&
    isRecord(next) &&
    typeof previous.revision === 'number' &&
    typeof next.revision === 'number' &&
    previous.revision > next.revision
    ? previous
    : next
}

export function applyUpdateStatus(client: QueryClient, status: UpdateStatus): void {
  client.setQueryData<UpdateStatus>(queryKeys.updates.status(), (previous) =>
    previous && previous.revision > status.revision ? previous : status
  )
}

export function useUpdateStatus(): UseQueryResult<UpdateStatus, Error> {
  const enabled = isDesktopTauri()
  const [, resetObserver] = useReducer((revision: number) => revision + 1, 0)
  useEffect(() => {
    if (!enabled) {
      return
    }
    window.addEventListener(DESKTOP_UPDATE_RESET_EVENT, resetObserver)
    return () => window.removeEventListener(DESKTOP_UPDATE_RESET_EVENT, resetObserver)
  }, [enabled])
  return useQuery({
    queryKey: queryKeys.updates.status(),
    queryFn: async ({ signal }) => {
      const status = await desktopService.getUpdateStatus()
      signal.throwIfAborted()
      return status
    },
    enabled,
    retry: false,
    staleTime: Infinity,
    structuralSharing: latestUpdateStatus,
  })
}

export function useUpdateConnection(): { state: UpdateConnection; connected: boolean; retryConnection: () => void } {
  const connection = useQuery<UpdateConnection>({
    queryKey: queryKeys.updates.connection(),
    queryFn: skipToken,
    initialData: 'connecting',
    staleTime: Infinity,
  })
  return {
    state: connection.data ?? 'connecting',
    connected: isDesktopTauri() && connection.data === 'connected',
    retryConnection: () => {
      if (isDesktopTauri()) {
        window.dispatchEvent(new Event(DESKTOP_UPDATE_RECONNECT_EVENT))
      }
    },
  }
}

export function useAutomaticUpdates(): UseMutationResult<UpdateStatus, Error, boolean> {
  const client = useQueryClient()
  return useMutation({
    mutationKey: [...queryKeys.updates.all, 'preference'],
    retry: false,
    mutationFn: async (enabled: boolean) => {
      if (!isDesktopTauri()) {
        throw new Error('Desktop updates are unavailable on this platform.')
      }
      return desktopService.setAutomaticUpdates(enabled)
    },
    onSuccess: (status) => applyUpdateStatus(client, status),
    onError: (error) => displayErrorNotification(error, "Couldn't save automatic updates"),
  })
}

type UpdateAction = { type: 'check' | 'download' } | { type: 'install'; version: string }
const actionKey = [...queryKeys.updates.all, 'action']

export function useUpdateActions(): UseMutationResult<UpdateStatus | undefined, Error, UpdateAction> & {
  submit: (action: UpdateAction) => void
} {
  const client = useQueryClient()
  const mutation = useMutation({
    mutationKey: actionKey,
    retry: false,
    mutationFn: async (action: UpdateAction) => {
      if (!isDesktopTauri()) {
        throw new Error('Desktop updates are unavailable on this platform.')
      }
      switch (action.type) {
        case 'check':
          return desktopService.checkForUpdates()
        case 'download':
          return desktopService.downloadUpdate()
        case 'install':
          await desktopService.installUpdate(action.version)
          return
      }
    },
    onSuccess: (status) => {
      if (status) {
        applyUpdateStatus(client, status)
      }
    },
    onError: (error, action) =>
      displayErrorNotification(
        error,
        action.type === 'install' ? "Couldn't install update" : "Couldn't update Pipedash"
      ),
  })

  return {
    ...mutation,
    submit: (action: UpdateAction) => {
      if (!isDesktopTauri() || client.isMutating({ mutationKey: actionKey }) > 0) {
        return
      }
      if (action.type === 'install' && client.getQueryData(queryKeys.updates.connection()) !== 'connected') {
        return
      }
      mutation.mutate(action)
    },
  }
}
