import { notifications } from '@mantine/notifications'

import { isMobileShell } from '../hooks/useIsMobile'
import { formatErrorMessage } from '../types/errors'

import classes from './errorDisplay.module.css'

export const NOTIFICATION_DURATIONS = {
  SUCCESS: 3000,
  ERROR: 5000,
  INFO: 4000,
  WARNING: 5000,
} as const

const compactToast = () =>
  isMobileShell() ? { classNames: { title: classes.title, description: classes.description } } : {}

export function displayErrorNotification(error: unknown, title = 'Error'): void {
  const message = formatErrorMessage(error)

  notifications.show({
    title,
    message,
    color: 'red',
    autoClose: NOTIFICATION_DURATIONS.ERROR,
    ...compactToast(),
  })
}

export function displaySuccessNotification(message: string, title = 'Success'): void {
  notifications.show({
    title,
    message,
    color: 'green',
    autoClose: NOTIFICATION_DURATIONS.SUCCESS,
    ...compactToast(),
  })
}
