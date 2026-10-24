import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { apiService } from '../services'
import { mobileService } from '../services/mobile'
import type { MobileDevicesResponse, PushStatus } from '../types'
import { displayErrorNotification, displaySuccessNotification } from '../utils/errorDisplay'

export function usePushStatus() {
  return useQuery<PushStatus>({
    queryKey: queryKeys.push.status(),
    queryFn: () => mobileService.getPushStatus(),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function usePairPushServer() {
  const queryClient = useQueryClient()

  return useMutation<PushStatus, unknown, { serverUrl: string; apiToken: string; label?: string }>({
    mutationFn: ({ serverUrl, apiToken, label }) => mobileService.pushPairServer(serverUrl, apiToken, label),
    onSuccess: (status) => {
      queryClient.setQueryData(queryKeys.push.status(), status)
      displaySuccessNotification('This device will get a push when a pipeline fails on that server.', 'Server paired')
    },
    onError: (error) => displayErrorNotification(error, "Couldn't pair server"),
  })
}

export function useUnpairPushServer() {
  const queryClient = useQueryClient()

  return useMutation<PushStatus, unknown, string>({
    mutationFn: (id) => mobileService.pushUnpairServer(id),
    onSuccess: (status) => queryClient.setQueryData(queryKeys.push.status(), status),
    onError: (error) => displayErrorNotification(error, "Couldn't remove server"),
  })
}

export function usePushSendTest() {
  return useMutation<void, unknown, string>({
    mutationFn: (id) => mobileService.pushSendTest(id),
    onSuccess: () => displaySuccessNotification('Check your notifications in a moment.', 'Test push sent'),
    onError: (error) => displayErrorNotification(error, "Couldn't send test push"),
  })
}

export function useMobileDevices() {
  return useQuery<MobileDevicesResponse>({
    queryKey: queryKeys.push.devices(),
    queryFn: () => apiService.listMobileDevices(),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useRemoveMobileDevice() {
  const queryClient = useQueryClient()

  return useMutation<void, unknown, string>({
    mutationFn: (id) => apiService.removeMobileDevice(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.push.devices() }),
    onError: (error) => displayErrorNotification(error, "Couldn't remove device"),
  })
}

export function useTestMobileDevice() {
  return useMutation<void, unknown, string>({
    mutationFn: (id) => apiService.testMobileDevice(id),
    onSuccess: () => displaySuccessNotification('The device should get it in a moment.', 'Test push sent'),
    onError: (error) => displayErrorNotification(error, "Couldn't send test push"),
  })
}
