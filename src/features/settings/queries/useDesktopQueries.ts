import {
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
  useIsMutating,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

import { logger } from '../../../lib/logger'
import { DESKTOP_STATUS_EVENT, desktopService } from '../../../services/desktop'
import { useNotificationStore } from '../../../stores/notificationStore'
import type { DesktopSettingsPatch, DesktopStatus } from '../../../types/domain'
import { isDesktopTauri } from '../../../utils/platform'

const statusKey = ['desktop', 'status'] as const
const mutationKey = ['desktop', 'settings'] as const

function newestStatus(client: QueryClient, status: DesktopStatus): DesktopStatus {
  const current = client.getQueryData<DesktopStatus>(statusKey)

  return current && current.revision > status.revision ? current : status
}

async function importPreference(status: DesktopStatus): Promise<DesktopStatus> {
  if (status.notification_preference_imported || status.error) {
    return status
  }
  if (!useNotificationStore.persist.hasHydrated()) {
    const { promise, resolve } = Promise.withResolvers<void>()
    const stop = useNotificationStore.persist.onFinishHydration(() => {
      stop()
      resolve()
    })
    await promise
  }

  return desktopService.importNotificationPreference(useNotificationStore.getState().notificationsEnabled)
}

export function refreshDesktopStatus(client: QueryClient): Promise<DesktopStatus> {
  return client.fetchQuery({
    queryKey: statusKey,
    queryFn: async () => {
      const status = newestStatus(client, await desktopService.getStatus())
      client.setQueryData(statusKey, status)
      const imported = await importPreference(status)

      return newestStatus(client, imported)
    },
    staleTime: 0,
    retry: false,
  })
}

export function useDesktopStatus(): UseQueryResult<DesktopStatus, Error> {
  return useQuery({
    queryKey: statusKey,
    queryFn: () => desktopService.getStatus(),
    enabled: false,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export function useUpdateDesktopSettings(): UseMutationResult<DesktopStatus, Error, DesktopSettingsPatch> & {
  busy: boolean
} {
  const client = useQueryClient()
  const busy = useIsMutating({ mutationKey }) > 0
  const mutation = useMutation({
    mutationKey,
    mutationFn: async (patch: DesktopSettingsPatch) => {
      const status = await desktopService.updateSettings(patch)
      client.setQueryData(statusKey, newestStatus(client, status))

      return Object.keys(patch).length === 0 ? importPreference(status) : status
    },
    onSuccess: (status) => client.setQueryData(statusKey, newestStatus(client, status)),
  })

  return { ...mutation, busy }
}

export function useDesktopLifecycle(): void {
  const client = useQueryClient()

  useEffect(() => {
    if (!isDesktopTauri()) {
      return
    }
    let active = true
    let remove: (() => void) | undefined

    listen<DesktopStatus>(DESKTOP_STATUS_EVENT, ({ payload }) => {
      if (active) {
        client.setQueryData(statusKey, newestStatus(client, payload))
      }
    })
      .then(async (unlisten) => {
        if (!active) {
          unlisten()
          return
        }
        remove = unlisten
        await refreshDesktopStatus(client)
      })
      .catch((error: unknown) => logger.warn('Desktop', "Couldn't load desktop integration settings", error))

    return () => {
      active = false
      remove?.()
    }
  }, [client])
}
