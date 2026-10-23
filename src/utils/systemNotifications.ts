import { logger } from '../lib/logger'
import { isTauri, tauriService } from '../services'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'

export const SYSTEM_NOTIFICATION_CLICKED_EVENT = 'system-notification-clicked'

export const openNotificationDetails = (id: string): void => {
  useNotificationCenterStore.getState().open({ type: 'notification', id })
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

export function showSystemNotification(title: string, body: string, notificationId: string): void {
  if (!systemNotificationsSupported() || Notification.permission !== 'granted') {
    return
  }

  if (isTauri()) {
    tauriService
      .showSystemNotification(title, body, notificationId)
      .catch((error: unknown) => logger.warn('systemNotifications', "Couldn't show notification", error))
    return
  }

  try {
    const notification = new Notification(title, { body })

    notification.onclick = () => {
      window.focus()
      openNotificationDetails(notificationId)
    }
  } catch (error) {
    logger.warn('systemNotifications', "Couldn't show notification", error)
  }
}
