import { notifications } from '@mantine/notifications'
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'

export function useRunHistory(pipelineId: string, page = 1, pageSize = 20) {
  return useQuery({
    queryKey: queryKeys.runs.list(pipelineId, page),
    queryFn: () => service.fetchRunHistory(pipelineId, page, pageSize),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    placeholderData: keepPreviousData,
    enabled: !!pipelineId,
  })
}

export function useTablePreferences(providerId: number, tableId: string) {
  return useQuery({
    queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
    queryFn: async () => {
      const prefsJson = await service.getTablePreferences(providerId, tableId)

      if (prefsJson) {
        try {
          return JSON.parse(prefsJson)
        } catch {
          return null
        }
      }

      const defaultPrefsJson = await service.getDefaultTablePreferences(providerId, tableId)

      if (defaultPrefsJson) {
        try {
          return JSON.parse(defaultPrefsJson)
        } catch {
          return null
        }
      }

      return null
    },
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.LONG,
    enabled: !!providerId && !!tableId,
  })
}

export function useSaveTablePreferences() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      providerId,
      tableId,
      preferences,
    }: {
      providerId: number
      tableId: string
      preferences: unknown
    }) => service.saveTablePreferences(providerId, tableId, JSON.stringify(preferences)),

    onMutate: async ({ providerId, tableId, preferences }) => {
      await queryClient.cancelQueries({
        queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
      })

      const previousPreferences = queryClient.getQueryData(
        queryKeys.tablePreferences.detail(providerId, tableId)
      )

      queryClient.setQueryData(queryKeys.tablePreferences.detail(providerId, tableId), preferences)

      return { previousPreferences, providerId, tableId }
    },

    onError: (error: Error, _variables, context) => {
      if (context?.previousPreferences !== undefined) {
        queryClient.setQueryData(
          queryKeys.tablePreferences.detail(context.providerId, context.tableId),
          context.previousPreferences
        )
      }

      notifications.show({
        title: 'Failed to Save Preferences',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },

    onSuccess: (_data, { providerId, tableId }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
      })

      notifications.show({
        title: 'Preferences Saved',
        message: 'Table preferences updated successfully',
        color: 'green',
        autoClose: 2000,
      })
    },
  })
}

export function useClearRunHistoryCache() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipelineId: string) => service.clearRunHistoryCache(pipelineId),

    onSuccess: (_data, pipelineId) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

      notifications.show({
        title: 'Cache Cleared',
        message: 'Run history cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Clear Cache',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}

export function useCancelRun() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      service.cancelPipelineRun(pipelineId, runNumber),

    onSuccess: (_data, { pipelineId, runNumber }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

      notifications.show({
        title: 'Run Cancelled',
        message: `Run #${runNumber} cancellation requested`,
        color: 'blue',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Failed to Cancel Run',
        message: error.message || 'Unknown error',
        color: 'red',
      })
    },
  })
}
