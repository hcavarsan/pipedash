import { isMobileShell } from '../hooks/useIsMobile'
import { haptic } from '../lib/haptics'
import { notifications } from '../lib/notifications'
import { formatErrorMessage } from '../types/errors'

import classes from './errorDisplay.module.css'

export const NOTIFICATION_DURATIONS = {
  SUCCESS: 3000,
  ERROR: 5000,
  ERROR_MOBILE: 8000,
  WARNING: 5000,
} as const

export interface NotificationActions {
  onRetry?: () => void
  onDetails?: () => void
}

let sequence = 0

export function errorMessage(error: unknown, fallback: string): string {
  const candidate =
    typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string'
      ? error.message
      : error
  const message = formatErrorMessage(candidate).trim()

  return !message || /^(an? )?unknown error( occurred)?[.!]?$/i.test(message) ? fallback : message
}

export function displayErrorNotification(
  error: unknown,
  title: string,
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
    message:
      onRetry || onDetails ? (
        <>
          {message}
          {onRetry ? (
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
          ) : null}
          {onDetails ? (
            <button
              type="button"
              className={classes.retry}
              onClick={(event) => {
                event.stopPropagation()
                notifications.hide(id)
                onDetails()
              }}
            >
              View details
            </button>
          ) : null}
        </>
      ) : (
        message
      ),
    autoClose: onRetry || onDetails ? false : duration,
  })
}

export function displaySuccessNotification(message: string, title: string): void {
  haptic('success')
  notifications.show({ title, message, color: 'green', autoClose: NOTIFICATION_DURATIONS.SUCCESS })
}

export function displayWarningNotification(message: string, title: string): void {
  haptic('warning')
  notifications.show({ title, message, color: 'orange', autoClose: NOTIFICATION_DURATIONS.WARNING })
}
