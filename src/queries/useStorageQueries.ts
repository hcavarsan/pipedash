import {
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { notifications } from '../lib/notifications'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services/service'
import { useAuthStore } from '../stores/authStore'
import type {
  ConfigContentResponse,
  MigrationOptions,
  MigrationPlan,
  MigrationResult,
  PipedashConfig,
  StorageConfigResponse,
  StoragePathsResponse,
} from '../types/domain'
import { getHttpStatus } from '../types/errors'
import { errorMessage } from '../utils/errorDisplay'
import { formatCount } from '../utils/formatCount'

export function useStorageConfig(options?: { enabled?: boolean }): UseQueryResult<StorageConfigResponse | null, Error> {
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

export function useStoragePaths(): UseQueryResult<StoragePathsResponse, Error> {
  return useQuery({
    queryKey: queryKeys.storage.paths(),
    queryFn: () => service.getStoragePaths(),
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.MEDIUM,
  })
}

export function useConfigContent(): UseQueryResult<ConfigContentResponse, Error> {
  return useQuery({
    queryKey: queryKeys.storage.configContent(),
    queryFn: () => service.getConfigContent(),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: false,
  })
}

export function useDefaultDataDir(options?: { enabled?: boolean }): UseQueryResult<string, Error> {
  return useQuery({
    queryKey: queryKeys.storage.defaultDataDir(),
    queryFn: () => service.getDefaultDataDir(),
    staleTime: STALE_TIMES.STATIC,
    gcTime: GC_TIMES.LONG,
    ...options,
  })
}

export function useSaveStorageConfig(): UseMutationResult<void, Error, PipedashConfig, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (config: PipedashConfig) => service.saveStorageConfig(config),

    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })

      void queryClient.refetchQueries({ queryKey: queryKeys.storage.config() })
      void queryClient.refetchQueries({ queryKey: queryKeys.storage.paths() })

      notifications.show({
        title: 'Configuration saved',
        message: 'Storage configuration updated successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't save configuration",
        message: errorMessage(error, 'Check the storage configuration and try again.'),
        color: 'red',
      })
    },
  })
}

export function useTestStorageConnection(): UseMutationResult<
  { success: boolean; message: string },
  Error,
  PipedashConfig,
  unknown
> {
  return useMutation({
    mutationFn: (config: PipedashConfig) => service.testStorageConnection(config),

    onSuccess: (result) => {
      if (!result.success) {
        notifications.show({
          title: "Couldn't connect to storage",
          message: result.message || 'Check the storage connection settings and try again.',
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
        title: "Couldn't connect to storage",
        message: errorMessage(error, 'Check the storage connection settings and try again.'),
        color: 'red',
      })
    },
  })
}

export function useCreateInitialConfig(): UseMutationResult<
  void,
  Error,
  { config: PipedashConfig; vaultPassword?: string },
  unknown
> {
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
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })

      await queryClient.refetchQueries({ queryKey: queryKeys.setup.status() })
      void queryClient.refetchQueries({ queryKey: queryKeys.storage.config() })
      void queryClient.refetchQueries({ queryKey: queryKeys.storage.paths() })

      notifications.show({
        title: 'Configuration created',
        message: 'Initial configuration saved successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't create configuration",
        message: errorMessage(error, 'Check the initial configuration and try again.'),
        color: 'red',
      })
    },
  })
}

export function usePlanStorageMigration(): UseMutationResult<
  MigrationPlan,
  Error,
  { config: PipedashConfig; options: MigrationOptions },
  unknown
> {
  return useMutation<MigrationPlan, Error, { config: PipedashConfig; options: MigrationOptions }>({
    mutationFn: ({ config, options }) => service.planStorageMigration(config, options),

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't plan storage migration",
        message: errorMessage(error, 'Check the migration settings and try again.'),
        color: 'red',
      })
    },
  })
}

export function useExecuteMigration(): UseMutationResult<
  MigrationResult,
  Error,
  { plan: MigrationPlan; options: MigrationOptions },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation<MigrationResult, Error, { plan: MigrationPlan; options: MigrationOptions }>({
    mutationFn: ({ plan, options }) => service.executeMigration(plan, options),

    onSuccess: (result) => {
      if (result.success) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
        void queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
        void queryClient.invalidateQueries({ queryKey: queryKeys.providers.all })
        void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

        void queryClient.refetchQueries({ queryKey: queryKeys.storage.config() })
        void queryClient.refetchQueries({ queryKey: queryKeys.storage.paths() })

        notifications.show({
          title: 'Migration complete',
          message: `Migrated ${formatCount(result.stats.providers_migrated, 'provider')}, ${formatCount(result.stats.tokens_migrated, 'token')} and ${formatCount(result.stats.metrics_migrated, 'metric entry', 'metric entries')}`,
          color: 'green',
        })
      } else {
        notifications.show({
          title: "Couldn't migrate storage",
          message: result.errors?.join(', ') || 'Migration completed with errors',
          color: 'red',
        })
      }
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't migrate storage",
        message: errorMessage(error, 'Check the storage connection and try again.'),
        color: 'red',
      })
    },
  })
}

function invalidateAfterConfigWrite(queryClient: QueryClient): void {
  void queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
  void queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
  void queryClient.invalidateQueries({ queryKey: queryKeys.providers.all })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
}

export function useSaveConfigContent(): UseMutationResult<void, Error, string, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (content: string) => service.saveConfigContent(content),
    onSuccess: () => invalidateAfterConfigWrite(queryClient),
  })
}

export function useMigrateAndSaveConfig(): UseMutationResult<
  MigrationResult,
  Error,
  { plan: MigrationPlan; options: MigrationOptions; content: string },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ plan, options, content }) => {
      const result = await service.executeMigration(plan, options)

      if (!result.success) {
        throw new Error(result.errors?.join(', ') || 'Migration completed with errors')
      }

      await service.saveConfigContent(content)

      return result
    },
    onSuccess: () => invalidateAfterConfigWrite(queryClient),
  })
}
