import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { notifications } from '../../../lib/notifications'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import { errorMessage } from '../../../utils/errorDisplay'

export interface TablePreferences {
  columnOrder?: string[] | undefined
  columnVisibility?: Record<string, boolean> | undefined
}

export function useTablePreferences(
  providerId: number,
  tableId: string
): UseQueryResult<TablePreferences | null, Error> {
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

export function useSaveTablePreferences(): UseMutationResult<
  void,
  Error,
  { providerId: number; tableId: string; preferences: unknown },
  { previousPreferences: unknown; providerId: number; tableId: string }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ providerId, tableId, preferences }: { providerId: number; tableId: string; preferences: unknown }) =>
      service.saveTablePreferences(providerId, tableId, JSON.stringify(preferences)),

    onMutate: async ({ providerId, tableId, preferences }) => {
      await queryClient.cancelQueries({
        queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
      })

      const previousPreferences = queryClient.getQueryData(queryKeys.tablePreferences.detail(providerId, tableId))

      queryClient.setQueryData(queryKeys.tablePreferences.detail(providerId, tableId), preferences)

      return { previousPreferences, providerId, tableId }
    },

    onError: (error: Error, _variables, context) => {
      if (context?.previousPreferences !== undefined) {
        queryClient.setQueryData(
          queryKeys.tablePreferences.detail(context.providerId, context.tableId),
          context.previousPreferences
        )
      } else if (context) {
        queryClient.removeQueries({
          queryKey: queryKeys.tablePreferences.detail(context.providerId, context.tableId),
          exact: true,
        })
      }

      notifications.show({
        title: "Couldn't save preferences",
        message: errorMessage(error, 'Try saving the table preferences again.'),
        color: 'red',
      })
    },

    onSuccess: (_data, { providerId, tableId }) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
      })

      notifications.show({
        title: 'Preferences saved',
        message: 'Table preferences updated successfully',
        color: 'green',
        autoClose: 2000,
      })
    },
  })
}
