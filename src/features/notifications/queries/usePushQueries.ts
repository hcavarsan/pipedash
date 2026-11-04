import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'

import { STALE_TIMES } from '../../../lib/cacheConfig'
import { queryKeys } from '../../../lib/queryKeys'
import { apiService } from '../../../services/api'
import type { MobileDevicesResponse, PushRelaySettings } from '../../../types/domain'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'

export function usePushRelay(): UseQueryResult<PushRelaySettings, Error> {
  return useQuery<PushRelaySettings>({
    queryKey: queryKeys.push.relay(),
    queryFn: () => apiService.getPushRelay(),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useSetPushRelay(): UseMutationResult<PushRelaySettings, unknown, boolean, unknown> {
  const queryClient = useQueryClient()

  return useMutation<PushRelaySettings, unknown, boolean>({
    mutationFn: (enabled) => apiService.setPushRelay(enabled),
    onSuccess: (settings) => {
      queryClient.setQueryData(queryKeys.push.relay(), settings)

      return queryClient.invalidateQueries({ queryKey: queryKeys.push.devices() })
    },
    onError: (error) => displayErrorNotification(error, "Couldn't change push alerts"),
  })
}

export function useMobileDevices(): UseQueryResult<MobileDevicesResponse, Error> {
  return useQuery<MobileDevicesResponse>({
    queryKey: queryKeys.push.devices(),
    queryFn: () => apiService.listMobileDevices(),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useRemoveMobileDevice(): UseMutationResult<void, unknown, string, unknown> {
  const queryClient = useQueryClient()

  return useMutation<void, unknown, string>({
    mutationFn: (id) => apiService.removeMobileDevice(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.push.devices() }),
    onError: (error) => displayErrorNotification(error, "Couldn't remove device"),
  })
}

export function useTestMobileDevice(): UseMutationResult<void, unknown, string, unknown> {
  return useMutation<void, unknown, string>({
    mutationFn: (id) => apiService.testMobileDevice(id),
    onSuccess: () => displaySuccessNotification('The device should get it in a moment.', 'Test push sent'),
    onError: (error) => displayErrorNotification(error, "Couldn't send test push"),
  })
}
