import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import { mobileService } from '../services/mobile'
import { useAuthStore } from '../stores/authStore'
import type { AppLockStatus, UnlockVaultResponse, VaultStatusResponse } from '../types'
import { getHttpStatus } from '../types/errors'
import { displayErrorNotification, errorMessage } from '../utils/errorDisplay'

export function useVaultPasswordStatus(options?: { enabled?: boolean }) {
  return useQuery({
    queryKey: queryKeys.vault.passwordStatus(),
    queryFn: () => service.getVaultPasswordStatus(),
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.MEDIUM,
    ...options,
  })
}

export function useVaultStatus(options?: { enabled?: boolean }) {
  return useQuery<VaultStatusResponse>({
    queryKey: queryKeys.vault.status(),
    queryFn: () => service.getVaultStatus(),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    ...options,
  })
}

async function unlockVaultSession(password: string): Promise<UnlockVaultResponse> {
  const { setToken, clearToken } = useAuthStore.getState()

  setToken(password)

  try {
    const result = await service.unlockVault(password)

    if (!result.success) {
      clearToken()
    }

    return result
  } catch (error) {
    clearToken()
    if (getHttpStatus(error) === 401) {
      return { success: false, message: 'Incorrect vault password' }
    }
    throw error
  }
}

export function useUnlockVault() {
  const queryClient = useQueryClient()

  return useMutation<UnlockVaultResponse, Error, string>({
    mutationFn: unlockVaultSession,
    onSuccess: async (data) => {
      if (data.success) {
        queryClient.invalidateQueries({ queryKey: queryKeys.setup.status() })
        await queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
      }
    },
  })
}

export function useLockVault() {
  const queryClient = useQueryClient()

  return useMutation<UnlockVaultResponse, Error, void>({
    mutationFn: () => service.lockVault(),
    onSuccess: () => {
      useAuthStore.getState().clearToken()
      queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
    },
    onError: (error) => {
      notifications.show({
        title: "Couldn't lock vault",
        message: errorMessage(error, 'Try locking the vault again.'),
        color: 'red',
      })
    },
  })
}

export function useUnlockWithBiometrics() {
  const queryClient = useQueryClient()

  return useMutation<UnlockVaultResponse, Error, void>({
    mutationFn: () => mobileService.unlockWithBiometrics(),
    onSuccess: async (data) => {
      if (data.success) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
      }
    },
  })
}

export function useSetAppLock() {
  const queryClient = useQueryClient()

  return useMutation<AppLockStatus, Error, { enabled: boolean; lockOnBackground: boolean }>({
    mutationFn: ({ enabled, lockOnBackground }) => mobileService.setAppLock(enabled, lockOnBackground),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.vault.status() }),
    onError: (error) => displayErrorNotification(error, "Couldn't update app lock"),
  })
}
