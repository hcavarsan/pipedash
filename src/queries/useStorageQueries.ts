import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import { useAuthStore } from '../stores/authStore'
import type { MigrationOptions, MigrationPlan, MigrationResult, PipedashConfig } from '../types'
import { getHttpStatus } from '../types/errors'
import { formatCount } from '../utils/formatCount'

export function useStorageConfig(options?: { enabled?: boolean }) {
  return useQuery({
    queryKey: queryKeys.storage.config(),
    queryFn: async () => {
      try {
        return await service.getStorageConfig()
      } catch (error) {
        if (getHttpStatus(error) === 404) {
          return null
        }
        throw error
      }
    },
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.MEDIUM,
    ...options,
  })
}

export function useStoragePaths() {
  return useQuery({
    queryKey: queryKeys.storage.paths(),
    queryFn: () => service.getStoragePaths(),
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.MEDIUM,
  })
}

export function useConfigContent() {
  return useQuery({
    queryKey: queryKeys.storage.configContent(),
    queryFn: () => service.getConfigContent(),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: false,
  })
}

export function useDefaultDataDir(options?: { enabled?: boolean }) {
  return useQuery({
    queryKey: queryKeys.storage.defaultDataDir(),
    queryFn: () => service.getDefaultDataDir(),
    staleTime: STALE_TIMES.STATIC,
    gcTime: GC_TIMES.LONG,
    ...options,
  })
}

export function useSaveStorageConfig() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (config: PipedashConfig) => service.saveStorageConfig(config),

    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })

      queryClient.refetchQueries({ queryKey: queryKeys.storage.config() })
      queryClient.refetchQueries({ queryKey: queryKeys.storage.paths() })

      notifications.show({
        title: 'Configuration saved',
        message: 'Storage configuration updated successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't save configuration",
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function useTestStorageConnection() {
  return useMutation({
    mutationFn: (config: PipedashConfig) => service.testStorageConnection(config),

    onSuccess: (result) => {
      if (!result.success) {
        notifications.show({
          title: 'Connection failed',
          message: result.message || 'Failed to connect to storage',
          color: 'red',
        })

        return
      }

      notifications.show({
        title: 'Connection successful',
        message: 'Storage connection test passed',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Connection failed',
        message: error.message || 'Failed to connect to storage',
        color: 'red',
      })
    },
  })
}

export function useCreateInitialConfig() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ config, vaultPassword }: { config: PipedashConfig; vaultPassword?: string }) => {
      await service.createInitialConfig(config, vaultPassword)

      if (vaultPassword) {
        useAuthStore.getState().setToken(vaultPassword)
      }

      await service.bootstrapApp()
    },

    onSuccess: async () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })

      await queryClient.refetchQueries({ queryKey: queryKeys.setup.status() })
      queryClient.refetchQueries({ queryKey: queryKeys.storage.config() })
      queryClient.refetchQueries({ queryKey: queryKeys.storage.paths() })

      notifications.show({
        title: 'Configuration created',
        message: 'Initial configuration saved successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't create configuration",
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function usePlanStorageMigration() {
  return useMutation<MigrationPlan, Error, { config: PipedashConfig; options: MigrationOptions }>({
    mutationFn: ({ config, options }) => service.planStorageMigration(config, options),

    onError: (error: Error) => {
      notifications.show({
        title: 'Migration planning failed',
        message: error.message || 'Failed to plan storage migration',
        color: 'red',
      })
    },
  })
}

export function useExecuteMigration() {
  const queryClient = useQueryClient()

  return useMutation<MigrationResult, Error, { plan: MigrationPlan; options: MigrationOptions }>({
    mutationFn: ({ plan, options }) => service.executeMigration(plan, options),

    onSuccess: (result) => {
      if (result.success) {
        queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
        queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
        queryClient.invalidateQueries({ queryKey: queryKeys.providers.all })
        queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

        queryClient.refetchQueries({ queryKey: queryKeys.storage.config() })
        queryClient.refetchQueries({ queryKey: queryKeys.storage.paths() })

        notifications.show({
          title: 'Migration complete',
          message: `Migrated ${formatCount(result.stats.providers_migrated, 'provider')} and ${formatCount(result.stats.tokens_migrated, 'token')}`,
          color: 'green',
        })
      } else {
        notifications.show({
          title: 'Migration failed',
          message: result.errors?.join(', ') || 'Migration completed with errors',
          color: 'red',
        })
      }
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Migration failed',
        message: error.message || 'Failed to execute migration',
        color: 'red',
      })
    },
  })
}
