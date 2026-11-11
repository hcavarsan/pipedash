import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

import { logger } from '../../../lib/logger'
import { desktopService, UPDATE_STATUS_EVENT } from '../../../services/desktop'
import type { UpdateStatus } from '../../../types/native'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isDesktopTauri } from '../../../utils/platform'

const statusKey = ['desktop', 'update'] as const

export function useUpdateStatus(): UseQueryResult<UpdateStatus, Error> {
  return useQuery({
    queryKey: statusKey,
    queryFn: () => desktopService.getUpdateStatus(),
    enabled: false,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export function useCheckForUpdate(): UseMutationResult<UpdateStatus, Error, void> {
  const client = useQueryClient()

  return useMutation<UpdateStatus, Error, void>({
    mutationFn: () => desktopService.checkForUpdate(),
    onSuccess: (status) => client.setQueryData(statusKey, status),
    onError: (error) => displayErrorNotification(error, "Couldn't check for updates"),
  })
}

export function useInstallUpdate(): UseMutationResult<void, Error, void> {
  return useMutation<void, Error, void>({
    mutationFn: () => desktopService.installUpdate(),
    onError: (error) => displayErrorNotification(error, "Couldn't install the update"),
  })
}

export function useUpdaterLifecycle(): void {
  const client = useQueryClient()

  useEffect(() => {
    if (!isDesktopTauri()) {
      return
    }
    let active = true
    let published = false
    let remove: (() => void) | undefined

    listen<UpdateStatus>(UPDATE_STATUS_EVENT, ({ payload }) => {
      if (active) {
        published = true
        client.setQueryData(statusKey, payload)
      }
    })
      .then(async (unlisten) => {
        if (!active) {
          unlisten()
          return
        }
        remove = unlisten
        const status = await desktopService.getUpdateStatus()
        if (active && !published) {
          client.setQueryData(statusKey, status)
        }
      })
      .catch((error: unknown) => logger.warn('Desktop', "Couldn't load the update status", error))

    return () => {
      active = false
      remove?.()
    }
  }, [client])
}
