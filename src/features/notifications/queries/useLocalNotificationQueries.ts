import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect } from 'react'

import { queryKeys } from '../../../lib/queryKeys'
import { hasLocalCore, isRemoteClient } from '../../../services/connection'
import { mobileService } from '../../../services/mobile'
import type { LocalNotificationSettings } from '../../../types/domain'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'
import { isMobilePlatform } from '../../../utils/platform'

const mutationKey = [...queryKeys.localNotifications.all, 'change']

export function useLocalNotificationSettings(): UseQueryResult<LocalNotificationSettings, Error> {
  return useQuery({
    queryKey: queryKeys.localNotifications.settings(),
    queryFn: () => mobileService.getLocalNotificationSettings(),
    enabled: isMobilePlatform() && (hasLocalCore() || isRemoteClient()),
    staleTime: 0,
    retry: false,
  })
}

export function useSetLocalNotifications(): UseMutationResult<LocalNotificationSettings, Error, boolean, void> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey,
    mutationFn: (enabled: boolean) => mobileService.setLocalNotificationsEnabled(enabled),
    onMutate: () => queryClient.cancelQueries({ queryKey: queryKeys.localNotifications.settings() }),
    onSuccess: (settings) => {
      queryClient.setQueryData(queryKeys.localNotifications.settings(), settings)
    },
    onError: (error, enabled) =>
      displayErrorNotification(error, enabled ? "Couldn't turn on notifications" : "Couldn't turn off notifications"),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() }),
  })
}

export function useRemoveAlertHost(): UseMutationResult<void, Error, void> {
  const queryClient = useQueryClient()

  return useMutation<void, Error, void>({
    mutationFn: () => mobileService.removeAlertHost(),
    onError: (error) => displayErrorNotification(error, "Couldn't remove the server"),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() }),
  })
}

export function useSendTestAlert(): UseMutationResult<void, Error, void> {
  return useMutation<void, Error, void>({
    mutationFn: () => mobileService.sendTestAlert(),
    onSuccess: () => displaySuccessNotification('Check your notifications in a moment.', 'Test alert sent'),
    onError: (error) => displayErrorNotification(error, "Couldn't send test alert"),
  })
}

export function useLocalNotificationsLifecycle(): void {
  const queryClient = useQueryClient()
  useEffect(() => {
    if (!(isMobilePlatform() && (hasLocalCore() || isRemoteClient()))) {
      return
    }
    const refresh = () => {
      if (document.visibilityState === 'hidden' || queryClient.isMutating({ mutationKey }) > 0) {
        return
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() })
    }
    document.addEventListener('visibilitychange', refresh)
    window.addEventListener('focus', refresh)

    return () => {
      document.removeEventListener('visibilitychange', refresh)
      window.removeEventListener('focus', refresh)
    }
  }, [queryClient])
}
