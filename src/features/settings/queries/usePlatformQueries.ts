import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query'
import { logger } from '../../../lib/logger'
import { notifications } from '../../../lib/notifications'

import { service } from '../../../services/service'
import { useLogViewerPrefs } from '../../../stores/logViewerPrefsStore'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import { useNotificationStore } from '../../../stores/notificationStore'
import { formatCount } from '../../../utils/formatCount'
import { isDesktopTauri } from '../../../utils/platform'
import { refreshDesktopNotifications } from '../../notifications/queries/useDesktopNotifications'
import { refreshDesktopStatus } from './useDesktopQueries'

interface FactoryResetResult {
  providers_removed: number
  caches_cleared: boolean
  tokens_cleared: boolean
  metrics_cleared: boolean
}

export function useFactoryReset(): UseMutationResult<FactoryResetResult, Error, void, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async () => {
      const result = await service.factoryReset()

      if (!result.caches_cleared || !result.tokens_cleared || !result.metrics_cleared) {
        throw new Error('Some credentials, cached data or metrics could not be deleted')
      }

      return result
    },

    onSuccess: (result) => {
      queryClient.clear()
      useNotificationStore.setState(useNotificationStore.getInitialState(), true)
      useLogViewerPrefs.setState(useLogViewerPrefs.getInitialState(), true)
      useNotificationCenterStore.setState({ items: [], selection: null })
      localStorage.clear()
      if (isDesktopTauri()) {
        void refreshDesktopStatus(queryClient).catch((error: unknown) =>
          logger.warn('Desktop', "Couldn't reload desktop settings after reset", error)
        )
        void refreshDesktopNotifications(queryClient).catch((error: unknown) =>
          logger.warn('Desktop', "Couldn't reload desktop notifications after reset", error)
        )
      }

      notifications.show({
        title: 'Factory reset complete',
        message: `Removed ${formatCount(result.providers_removed, 'provider')} and cleared app preferences. Storage configuration, backups and exports were kept.`,
        color: 'green',
      })
    },

    onError: (error: Error) => {
      void queryClient.invalidateQueries()
      notifications.show({
        title: 'Factory reset incomplete',
        message: `${error.message || 'Failed to perform factory reset'}. Some data may already be deleted. Fix the error and retry.`,
        color: 'red',
      })
    },
  })
}
