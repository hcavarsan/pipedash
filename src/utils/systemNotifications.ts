import { getCurrentWindow } from '@tauri-apps/api/window'

import { logger } from '../lib/logger'
import { isTauri } from '../services'

export function focusAppWindow(): void {
  if (!isTauri()) {
    window.focus()
    return
  }

  getCurrentWindow()
    .setFocus()
    .catch((error: unknown) => logger.warn('systemNotifications', "Couldn't focus the window", error))
}

export const systemNotificationsSupported = (): boolean =>
  'Notification' in window && (isTauri() || window.isSecureContext)

export async function requestSystemNotificationPermission(): Promise<boolean> {
  if (!systemNotificationsSupported()) {
    return false
  }

  if (Notification.permission === 'granted') {
    return true
  }

  return (await Notification.requestPermission()) === 'granted'
}

export function showSystemNotification(title: string, body: string, onClick?: () => void): void {
  if (!systemNotificationsSupported() || Notification.permission !== 'granted') {
    return
  }

  try {
    const notification = new Notification(title, { body })

    if (onClick) {
      notification.onclick = onClick
    }
  } catch (error) {
    logger.warn('systemNotifications', "Couldn't show notification", error)
  }
}
