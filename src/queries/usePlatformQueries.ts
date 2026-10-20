import { notifications } from '@mantine/notifications'
import { useMutation, useQueryClient } from '@tanstack/react-query'

import { service } from '../services'
import { formatCount } from '../utils/formatCount'

export function useFactoryReset() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.factoryReset(),

    onSuccess: (result) => {
      queryClient.clear()
      localStorage.clear()

      notifications.show({
        title: 'Factory reset complete',
        message: `Removed ${formatCount(result.providers_removed, 'provider')} and cleared all local settings.`,
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't reset the application",
        message: error.message || 'Failed to perform factory reset',
        color: 'red',
      })
    },
  })
}
