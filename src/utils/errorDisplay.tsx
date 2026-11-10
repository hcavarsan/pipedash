import { IconAlertCircle, IconAlertTriangle, IconCircleCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'

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

interface NotificationActionProps {
  notificationId: string
  children: ReactNode
  onAction: () => void
}

function NotificationAction({ notificationId, children, onAction }: NotificationActionProps): ReactNode {
  return (
    <button
      type="button"
      className={classes.retry}
      onClick={(event) => {
        event.stopPropagation()
        notifications.hide(notificationId)
        onAction()
      }}
    >
      {children}
    </button>
  )
}

export function displayErrorNotification(
  error: unknown,
  title: string,
  { onRetry, onDetails }: NotificationActions = {}
): void {
  const id = `error-${++sequence}`
  const message = errorMessage(error, 'Try again.')
  const duration = isMobileShell() ? NOTIFICATION_DURATIONS.ERROR_MOBILE : NOTIFICATION_DURATIONS.ERROR

  const hasActions = Boolean(onRetry || onDetails)
  let body: ReactNode = message

  if (hasActions) {
    body = (
      <>
        {message}
        {onRetry && (
          <NotificationAction notificationId={id} onAction={onRetry}>
            Retry
          </NotificationAction>
        )}
        {onDetails && (
          <NotificationAction notificationId={id} onAction={onDetails}>
            View details
          </NotificationAction>
        )}
      </>
    )
  }

  haptic('error')
  notifications.show({
    id,
    title,
    color: 'red',
    icon: <IconAlertCircle size={16} aria-hidden />,
    message: body,
    autoClose: hasActions ? false : duration,
  })
}

export function displaySuccessNotification(message: string, title: string): void {
  haptic('success')
  notifications.show({
    title,
    message,
    color: 'green',
    icon: <IconCircleCheck size={16} aria-hidden />,
    autoClose: NOTIFICATION_DURATIONS.SUCCESS,
  })
}

export function displayWarningNotification(message: string, title: string): void {
  haptic('warning')
  notifications.show({
    title,
    message,
    color: 'orange',
    icon: <IconAlertTriangle size={16} aria-hidden />,
    autoClose: NOTIFICATION_DURATIONS.WARNING,
  })
}
