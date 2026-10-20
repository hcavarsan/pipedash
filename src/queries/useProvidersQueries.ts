import { notifications } from '@mantine/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { ProviderConfig, ProviderSummary } from '../types'
import { invalidateProviderPipelines, setProviderPipelines } from './usePipelinesQueries'

export function useProviders(options?: { enabled?: boolean }) {
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

export function useAddProvider() {
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
                last_fetch_at: null,
                configured_repositories: [],
              },
            ]
      )

      service
        .fetchPipelines(newProviderId)
        .then((pipelines) => setProviderPipelines(queryClient, newProviderId, pipelines))
        .catch((error) => {
          logger.error('useAddProvider', 'Initial pipeline fetch failed', error)
          invalidateProviderPipelines(queryClient, newProviderId)
        })

      queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })

      notifications.show({
        title: 'Provider added',
        message: 'Fetching pipelines...',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't add provider",
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function useUpdateProvider() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, config }: { id: number; config: ProviderConfig }) => service.updateProvider(id, config),

    onMutate: async ({ id, config }) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.providers.list() })

      const previousProviders = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list())

      queryClient.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), (old = []) =>
        old.map((p) => (p.id === id ? { ...p, ...config } : p))
      )

      return { previousProviders }
    },

    onError: (error: Error, _vars, context) => {
      if (context?.previousProviders) {
        queryClient.setQueryData(queryKeys.providers.list(), context.previousProviders)
      }

      notifications.show({
        title: "Couldn't update provider",
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },

    onSuccess: (_data, { id }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.detail(id) })
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      invalidateProviderPipelines(queryClient, id)

      notifications.show({
        title: 'Provider updated',
        message: 'Refreshing pipelines...',
        color: 'green',
      })
    },
  })
}

export function useRemoveProvider() {
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

      queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.list(), exact: true })

      notifications.show({
        title: 'Provider removed',
        message: 'All associated data cleared',
        color: 'gray',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't remove provider",
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function useUpdateProviderRefreshInterval() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, refreshInterval }: { id: number; refreshInterval: number }) =>
      service.updateProviderRefreshInterval(id, refreshInterval),

    onMutate: async ({ id, refreshInterval }) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.providers.list() })

      const previousProviders = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list())

      queryClient.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), (old = []) =>
        old.map((p) => (p.id === id ? { ...p, refresh_interval: refreshInterval } : p))
      )

      return { previousProviders }
    },

    onError: (error: Error, _vars, context) => {
      if (context?.previousProviders) {
        queryClient.setQueryData(queryKeys.providers.list(), context.previousProviders)
      }

      notifications.show({
        title: "Couldn't update refresh interval",
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },

    onSuccess: (_data, { id }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.detail(id) })
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.refreshInterval(id) })

      notifications.show({
        title: 'Refresh interval updated',
        message: 'Provider will use the new refresh interval',
        color: 'green',
      })
    },
  })
}

export function useProviderPermissions(providerType: string, config: ProviderConfig) {
  return useQuery({
    queryKey: ['checkProviderPermissions', providerType, config.config],
    queryFn: () => service.checkProviderPermissions(providerType, config.config || {}),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled: false,
  })
}
