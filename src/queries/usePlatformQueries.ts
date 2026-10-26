import { notifications } from '@mantine/notifications'
import { useMutation, useQueryClient } from '@tanstack/react-query'

import { service } from '../services'
import { useNotificationStore } from '../stores/notificationStore'
import { usePinnedStore } from '../stores/pinnedStore'
import { formatCount } from '../utils/formatCount'

export function useFactoryReset() {
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
      usePinnedStore.setState(usePinnedStore.getInitialState(), true)
      localStorage.clear()

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
