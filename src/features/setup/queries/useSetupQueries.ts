import { type UseMutationResult, useMutation } from '@tanstack/react-query'
import { logger } from '../../../lib/logger'
import { service } from '../../../services/service'

export function useRestartApp(): UseMutationResult<void, Error, void, unknown> {
  return useMutation({
    mutationFn: () => service.restartApp(),
    onError: (error) => logger.error('SuccessStep', 'Failed to restart app', error),
  })
}
