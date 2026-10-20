import { isTauri } from '../services'

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

export function showSystemNotification(title: string, body: string): void {
  if (systemNotificationsSupported() && Notification.permission === 'granted') {
    new Notification(title, { body })
  }
}
