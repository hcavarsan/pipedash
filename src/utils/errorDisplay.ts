import { notifications } from '@mantine/notifications'

import { formatErrorMessage } from '../types/errors'

export const NOTIFICATION_DURATIONS = {
  SUCCESS: 3000,
  ERROR: 5000,
  INFO: 4000,
  WARNING: 5000,
} as const

export function displayErrorNotification(
  error: unknown,
  title = 'Error'
): void {
  const message = formatErrorMessage(error)

  notifications.show({
    title,
    message,
    color: 'red',
    autoClose: NOTIFICATION_DURATIONS.ERROR,
  })
}

export function displaySuccessNotification(
  message: string,
  title = 'Success'
): void {
  notifications.show({
    title,
    message,
    color: 'green',
    autoClose: NOTIFICATION_DURATIONS.SUCCESS,
  })
}


