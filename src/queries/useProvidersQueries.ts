import { notifications } from '@mantine/notifications'
import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services/service'
import type { PermissionCheckResult, ProviderConfig, ProviderImportResult, ProviderSummary } from '../types/domain'
import { errorMessage } from '../utils/errorDisplay'
import { saveFile } from '../utils/saveFile'
import { invalidateProviderPipelines } from './usePipelinesQueries'

export function useProviders(options?: { enabled?: boolean }): UseQueryResult<ProviderSummary[], Error> {
  return useQuery({
    queryKey: queryKeys.providers.list(),
    queryFn: async () => {
      logger.debug('useProviders', 'Fetching providers...')
      const data = await service.listProviders()

      logger.debug('useProviders', 'Received providers', { count: data.length })

      return data
    },
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
    enabled: options?.enabled ?? true,
  })
}

export function useAddProvider(): UseMutationResult<number, Error, ProviderConfig, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (config: ProviderConfig) => service.addProvider(config),

    onSuccess: async (newProviderId, config) => {
      queryClient.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), (old = []) =>
        old.some((provider) => provider.id === newProviderId)
          ? old
          : [
              ...old,
              {
                id: newProviderId,
                name: config.name || `${config.provider_type} Provider`,
                provider_type: config.provider_type,
                icon: null,
                pipeline_count: 0,
                last_updated: null,
                refresh_interval: config.refresh_interval,
                last_fetch_status: 'never',
                last_fetch_error: null,
                last_fetch_error_kind: null,
                last_fetch_at: null,
                last_success_at: null,
                paused_until: null,
                freshness: { state: 'never', stale_at: null },
                recovery: null,
                health: {
                  level: 'operational',
                  signals: [],
                  next_attempt_at: null,
                  in_flight: false,
                  status_page: null,
                },
                configured_repositories: [],
              },
            ]
      )

      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      notifications.show({
        title: 'Provider added',
        message: 'Fetching pipelines...',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't add provider",
        message: errorMessage(error, 'Check the provider credentials and try again.'),
        color: 'red',
      })
    },
  })
}

export function useUpdateProvider(): UseMutationResult<
  void,
  Error,
  { id: number; config: ProviderConfig },
  { previousProviders: ProviderSummary[] | undefined }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, config }: { id: number; config: ProviderConfig }) => service.updateProvider(id, config),

    onMutate: async ({ id, config }) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.providers.list() })

      const previousProviders = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list())

      queryClient.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), (old) =>
        old?.map((p) => (p.id === id ? { ...p, name: config.name, refresh_interval: config.refresh_interval } : p))
      )

      return { previousProviders }
    },

    onError: (error: Error, _vars, context) => {
      if (context?.previousProviders) {
        queryClient.setQueryData(queryKeys.providers.list(), context.previousProviders)
      }

      notifications.show({
        title: "Couldn't update provider",
        message: errorMessage(error, 'Check the provider settings and try again.'),
        color: 'red',
      })
    },

    onSuccess: (_data, { id }) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.detail(id) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      invalidateProviderPipelines(queryClient, id)

      notifications.show({
        title: 'Provider updated',
        message: 'Refreshing pipelines...',
        color: 'green',
      })
    },
  })
}

export function useRemoveProvider(): UseMutationResult<void, Error, number, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: number) => service.removeProvider(id),

    onSuccess: (_data, id) => {
      queryClient.removeQueries({ queryKey: queryKeys.providers.detail(id) })
      queryClient.removeQueries({ queryKey: queryKeys.providers.features(id) })
      queryClient.removeQueries({ queryKey: queryKeys.providers.permissions(id) })
      queryClient.removeQueries({ queryKey: queryKeys.providers.schema(id) })
      queryClient.removeQueries({ queryKey: queryKeys.providers.refreshInterval(id) })

      queryClient.removeQueries({ queryKey: queryKeys.pipelines.list({ providerId: id }), exact: true })
      queryClient.removeQueries({ queryKey: queryKeys.tableSchema.detail(id) })
      queryClient.removeQueries({
        predicate: (query) => query.queryKey[0] === 'tablePreferences' && query.queryKey[1] === id,
      })

      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      notifications.show({
        title: 'Provider removed',
        message: 'All associated data cleared',
        color: 'gray',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't remove provider",
        message: errorMessage(error, 'Try removing the provider again.'),
        color: 'red',
      })
    },
  })
}

export function useExportProviders(): UseMutationResult<boolean, Error, void, unknown> {
  return useMutation({
    mutationFn: async () =>
      saveFile('pipedash-providers.toml', await service.exportProviders(), {
        name: 'TOML File',
        extension: 'toml',
        mimeType: 'application/toml',
      }),

    onSuccess: (saved) => {
      if (saved) {
        notifications.show({
          title: 'Providers exported',
          message: 'Tokens are not in the file. Set each token variable on the other machine.',
          color: 'green',
        })
      }
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't export providers",
        message: errorMessage(error, 'Try exporting the providers again.'),
        color: 'red',
      })
    },
  })
}

function importSummary({ added, skipped }: ProviderImportResult): string {
  const parts = []

  if (added.length > 0) {
    parts.push(`Added ${added.join(', ')}. Add each token in the provider's edit dialog.`)
  }
  if (skipped.length > 0) {
    parts.push(`Skipped ${skipped.join(', ')}, already set up here.`)
  }

  return parts.join(' ')
}

export function useImportProviders(): UseMutationResult<ProviderImportResult, Error, File, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (file: File) => service.importProviders(await file.text()),

    onSuccess: (result) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      notifications.show({
        title: result.added.length > 0 ? 'Providers imported' : 'No providers imported',
        message: importSummary(result),
        color: result.added.length > 0 ? 'green' : 'gray',
        autoClose: 10_000,
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't import providers",
        message: errorMessage(error, 'Check the providers file and try again.'),
        color: 'red',
      })
    },
  })
}

export function useUpdateProviderRefreshInterval(
  notify = true
): UseMutationResult<
  void,
  Error,
  { id: number; refreshInterval: number },
  { previousProviders: ProviderSummary[] | undefined }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, refreshInterval }: { id: number; refreshInterval: number }) =>
      service.updateProviderRefreshInterval(id, refreshInterval),

    onMutate: async ({ id, refreshInterval }) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.providers.list() })

      const previousProviders = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list())

      queryClient.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), (old) =>
        old?.map((p) => (p.id === id ? { ...p, refresh_interval: refreshInterval } : p))
      )

      return { previousProviders }
    },

    onError: (error: Error, _vars, context) => {
      if (context?.previousProviders) {
        queryClient.setQueryData(queryKeys.providers.list(), context.previousProviders)
      }

      if (!notify) {
        return
      }
      notifications.show({
        title: "Couldn't update refresh interval",
        message: errorMessage(error, 'Try saving the refresh interval again.'),
        color: 'red',
      })
    },

    onSuccess: (_data, { id }) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.detail(id) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.refreshInterval(id) })
      if (!notify) {
        return
      }

      notifications.show({
        title: 'Refresh interval updated',
        message: 'Provider will use the new refresh interval',
        color: 'green',
      })
    },
  })
}

export function useProviderPermissions(
  providerType: string,
  config: ProviderConfig
): UseQueryResult<PermissionCheckResult, Error> {
  return useQuery({
    queryKey: ['checkProviderPermissions', providerType, config.config],
    queryFn: () => service.checkProviderPermissions(providerType, config.config || {}),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: false,
  })
}
