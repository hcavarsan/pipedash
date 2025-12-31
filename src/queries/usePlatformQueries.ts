import { notifications } from '@mantine/notifications'
import { useMutation, useQueryClient } from '@tanstack/react-query'

import { service } from '../services'

export function useFactoryReset() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => service.factoryReset(),

    onSuccess: () => {
      queryClient.clear()

      notifications.show({
        title: 'Factory Reset Complete',
        message: 'Application has been reset to initial state',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: 'Factory Reset Failed',
        message: error.message || 'Failed to perform factory reset',
        color: 'red',
      })
    },
  })
}
