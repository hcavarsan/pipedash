import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect } from 'react'

import { queryKeys } from '../../../lib/queryKeys'
import { mobileService } from '../../../services/mobile'
import type { LocalNotificationSettings } from '../../../types/domain'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isIosPlatform } from '../../../utils/platform'

const mutationKey = [...queryKeys.localNotifications.all, 'change']

export function useLocalNotificationSettings(): UseQueryResult<LocalNotificationSettings, Error> {
  return useQuery({
    queryKey: queryKeys.localNotifications.settings(),
    queryFn: () => mobileService.getLocalNotificationSettings(),
    enabled: isIosPlatform(),
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

export function useLocalNotificationsLifecycle(): void {
  const queryClient = useQueryClient()
  useEffect(() => {
    if (!isIosPlatform()) {
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
