import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import { useAuthStore } from '../stores/authStore'
import type { UnlockVaultResponse, VaultStatusResponse } from '../types'

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
    throw error
  }
}

export function useUnlockVault() {
  const queryClient = useQueryClient()

  return useMutation<UnlockVaultResponse, Error, string>({
    mutationFn: unlockVaultSession,
    onSuccess: (data) => {
      if (data.success) {
        queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
        queryClient.invalidateQueries({ queryKey: queryKeys.setup.status() })
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
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}
