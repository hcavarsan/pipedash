import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { GeneralConfig } from '../../../types/storage'
import { displayErrorNotification } from '../../../utils/errorDisplay'

export function useGeneralSettings(): UseQueryResult<GeneralConfig, Error> {
  return useQuery({
    queryKey: queryKeys.storage.general(),
    queryFn: () => service.getGeneralSettings(),
  })
}

export function useUpdateGeneralSettings({
  toastErrors = true,
}: {
  toastErrors?: boolean
} = {}): UseMutationResult<void, Error, GeneralConfig, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (update: GeneralConfig) => service.updateGeneralSettings(update),
    onSuccess: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.storage.general() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.storage.config() }),
      ]),
    onError: (error) => {
      logger.error('GeneralSection', 'Failed to update general settings', error)

      if (toastErrors) {
        displayErrorNotification(error, "Couldn't save general settings")
      }
    },
  })
}
