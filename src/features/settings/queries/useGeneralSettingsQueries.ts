import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query'
import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { GeneralConfig } from '../../../types/storage'
import { displayErrorNotification } from '../../../utils/errorDisplay'

export function useUpdateGeneralSettings(): UseMutationResult<void, Error, GeneralConfig, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (update: GeneralConfig) => service.updateGeneralSettings(update),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.storage.config() }),
    onError: (error) => {
      logger.error('GeneralSection', 'Failed to update general settings', error)
      displayErrorNotification(error, "Couldn't save general settings")
    },
  })
}
