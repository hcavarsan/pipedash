import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { notifications } from '../lib/notifications'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services/service'
import type { PermissionCheckResult } from '../types/plugins'
import type { ProviderDraft } from '../types/provider_draft'
import type { ProviderConfig, ProviderImportResult, ProviderSummary } from '../types/providers'
import { displayErrorNotification, displaySuccessNotification } from '../utils/errorDisplay'
import { saveFile } from '../utils/saveFile'

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

export function useSaveProviderDraft(): UseMutationResult<number, Error, ProviderDraft, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (draft: ProviderDraft) => service.saveProviderDraft(draft),

    onSuccess: (id, draft) => {
      const updated = draft.provider_id !== null

      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.detail(id) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      displaySuccessNotification(
        updated ? 'Changes saved.' : 'Pipelines appear here when the first fetch finishes.',
        updated ? 'Provider updated' : 'Provider added'
      )
    },

    onError: (error: Error, draft) => {
      displayErrorNotification(error, draft.provider_id === null ? "Couldn't add provider" : "Couldn't update provider")
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
      displayErrorNotification(error, "Couldn't remove provider")
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
        displaySuccessNotification(
          'Tokens are not in the file. Set each token variable on the other machine.',
          'Providers exported'
        )
      }
    },

    onError: (error: Error) => {
      displayErrorNotification(error, "Couldn't export providers")
    },
  })
}

function importSummary({ added, skipped, failed, not_attempted }: ProviderImportResult): string {
  const parts = []

  if (added.length > 0) {
    parts.push(`Added ${added.join(', ')}. Add each token in the provider's edit dialog.`)
  }
  if (skipped.length > 0) {
    parts.push(`Skipped ${skipped.join(', ')}, already set up here.`)
  }
  for (const { name, error } of failed) {
    parts.push(`Couldn't add ${name}: ${error}.`)
  }
  if (not_attempted.length > 0) {
    parts.push(`Not attempted: ${not_attempted.join(', ')}.`)
  }

  return parts.join(' ')
}

function importOutcome({ added, failed }: ProviderImportResult): { title: string; color: string } {
  if (added.length === 0) {
    return { title: 'No providers imported', color: failed.length > 0 ? 'orange' : 'gray' }
  }

  return failed.length > 0
    ? { title: "Some providers weren't imported", color: 'orange' }
    : { title: 'Providers imported', color: 'green' }
}

export function useImportProviders(): UseMutationResult<ProviderImportResult, Error, File, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (file: File) => service.importProviders(await file.text()),

    onSuccess: (result) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      notifications.show({ ...importOutcome(result), message: importSummary(result), autoClose: 10_000 })
    },

    onError: (error: Error) => {
      displayErrorNotification(error, "Couldn't import providers")
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
      displayErrorNotification(error, "Couldn't update refresh interval")
    },

    onSuccess: (_data, { id }) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.detail(id) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.refreshInterval(id) })
      if (!notify) {
        return
      }

      displaySuccessNotification('Provider will use the new refresh interval', 'Refresh interval updated')
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
