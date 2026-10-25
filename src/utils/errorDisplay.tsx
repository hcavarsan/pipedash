import { notifications } from '@mantine/notifications'

import { isMobileShell } from '../hooks/useIsMobile'
import { haptic } from '../lib/haptics'
import { formatErrorMessage } from '../types/errors'

import classes from './errorDisplay.module.css'

export const NOTIFICATION_DURATIONS = {
  SUCCESS: 3000,
  ERROR: 5000,
  ERROR_MOBILE: 8000,
  INFO: 4000,
  WARNING: 5000,
} as const

export interface NotificationActions {
  onRetry?: () => void
  onDetails?: () => void
}

let sequence = 0

export function displayErrorNotification(
  error: unknown,
  title = 'Error',
  { onRetry, onDetails }: NotificationActions = {}
): void {
  const id = `error-${++sequence}`
  const message = formatErrorMessage(error)
  const duration = isMobileShell() ? NOTIFICATION_DURATIONS.ERROR_MOBILE : NOTIFICATION_DURATIONS.ERROR

  haptic('error')
  notifications.show({
    id,
    title,
    color: 'red',
    message: onRetry ? (
      <>
        {message}
        <button
          type="button"
          className={classes.retry}
          onClick={(event) => {
            event.stopPropagation()
            notifications.hide(id)
            onRetry()
          }}
        >
          Retry
        </button>
      </>
    ) : (
      message
    ),
    autoClose: onRetry || onDetails ? false : duration,
    onClick: onDetails
      ? () => {
          notifications.hide(id)
          onDetails()
        }
      : undefined,
  })
}

export function displaySuccessNotification(message: string, title = 'Success'): void {
  haptic('success')
  notifications.show({ title, message, color: 'green', autoClose: NOTIFICATION_DURATIONS.SUCCESS })
}

export function displayWarningNotification(message: string, title = 'Warning'): void {
  haptic('warning')
  notifications.show({ title, message, color: 'orange', autoClose: NOTIFICATION_DURATIONS.WARNING })
}
