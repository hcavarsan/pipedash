import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'

import { STALE_TIMES } from '../../../lib/cacheConfig'
import { queryKeys } from '../../../lib/queryKeys'
import { type ConnectionInfo, connectionService, sharingService } from '../../../services/connection'
import { mobileService } from '../../../services/mobile'
import { service } from '../../../services/service'
import type { AlertHostInfo, PairedDevice, PairingCode, SharingStatus } from '../../../types/domain'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'

export interface PairInput {
  address: string
  code: string | null
  allowPrivateHttp: boolean
}

export function usePairWithServer(): UseMutationResult<ConnectionInfo, unknown, PairInput> {
  return useMutation<ConnectionInfo, unknown, PairInput>({
    mutationFn: ({ address, code, allowPrivateHttp }) => connectionService.pair(address, code, allowPrivateHttp),
  })
}

export function usePairAlertHost(): UseMutationResult<AlertHostInfo, unknown, PairInput> {
  return useMutation<AlertHostInfo, unknown, PairInput>({
    mutationFn: ({ address, code, allowPrivateHttp }) => mobileService.pairAlertHost(address, code, allowPrivateHttp),
  })
}

export function useRunOnThisDevice(): UseMutationResult<ConnectionInfo, unknown, void> {
  return useMutation<ConnectionInfo, unknown, void>({
    mutationFn: () => connectionService.runOnThisDevice(),
    onError: (error) => displayErrorNotification(error, "Couldn't set up Pipedash on this device"),
  })
}

export function useDisconnectServer(): UseMutationResult<ConnectionInfo, unknown, void> {
  return useMutation<ConnectionInfo, unknown, void>({
    mutationFn: () => connectionService.disconnect(),
    onSuccess: () => window.location.reload(),
    onError: (error) => displayErrorNotification(error, "Couldn't disconnect"),
  })
}

export function useSetAllowPrivateHttp(): UseMutationResult<void, unknown, boolean> {
  return useMutation<void, unknown, boolean>({
    mutationFn: (enabled) => connectionService.setAllowPrivateHttp(enabled),
    onError: (error) => displayErrorNotification(error, "Couldn't change the connection setting"),
  })
}

export function usePairedDevices({ enabled }: { enabled: boolean }): UseQueryResult<PairedDevice[], Error> {
  return useQuery<PairedDevice[]>({
    queryKey: queryKeys.devices.list(),
    queryFn: () => service.listPairedDevices(),
    enabled,
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useCreatePairingCode(): UseMutationResult<PairingCode, unknown, void> {
  return useMutation<PairingCode, unknown, void>({
    mutationFn: () => service.createPairingCode(),
  })
}

export function useRemovePairedDevice(): UseMutationResult<void, unknown, Pick<PairedDevice, 'id' | 'name'>> {
  const queryClient = useQueryClient()

  return useMutation<void, unknown, Pick<PairedDevice, 'id' | 'name'>>({
    mutationFn: ({ id }) => service.removePairedDevice(id),
    onSuccess: (_result, { name }) => {
      displaySuccessNotification(`${name} no longer has access.`, 'Device removed')

      return queryClient.invalidateQueries({ queryKey: queryKeys.devices.all })
    },
    onError: (error) => displayErrorNotification(error, "Couldn't remove device"),
  })
}

export function useSharingStatus({ enabled }: { enabled: boolean }): UseQueryResult<SharingStatus, Error> {
  return useQuery<SharingStatus>({
    queryKey: queryKeys.sharing.status(),
    queryFn: () => sharingService.getStatus(),
    enabled,
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useSetSharing(): UseMutationResult<SharingStatus, unknown, boolean> {
  const queryClient = useQueryClient()

  return useMutation<SharingStatus, unknown, boolean>({
    mutationFn: (enabled) => sharingService.setEnabled(enabled),
    onSuccess: (status) => queryClient.setQueryData(queryKeys.sharing.status(), status),
    onError: (error) => displayErrorNotification(error, "Couldn't change sharing"),
  })
}

export function useResetSharingPort(): UseMutationResult<SharingStatus, unknown, void> {
  const queryClient = useQueryClient()

  return useMutation<SharingStatus, unknown, void>({
    mutationFn: () => sharingService.resetPort(),
    onSuccess: (status) => queryClient.setQueryData(queryKeys.sharing.status(), status),
    onError: (error) => displayErrorNotification(error, "Couldn't reset the sharing port"),
  })
}
