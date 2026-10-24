import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification'

import { logger } from '../lib/logger'
import { isTauri, tauriService } from '../services'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'

import { isMobilePlatform } from './platform'

export const SYSTEM_NOTIFICATION_CLICKED_EVENT = 'system-notification-clicked'

export const openNotificationDetails = (key: string): void => {
  useNotificationCenterStore.getState().open({ type: 'notification', key })
}

export const systemNotificationsSupported = (): boolean =>
  isTauri() || ('Notification' in window && window.isSecureContext)

export function notificationProblem(kind: 'unsupported' | 'blocked'): string {
  if (isMobilePlatform()) {
    return kind === 'unsupported'
      ? "Notifications aren't available on this device."
      : 'Notifications are off for Pipedash. Turn them on in the Settings app under Notifications > Pipedash, then try again.'
  }

  return kind === 'unsupported'
    ? 'Browsers allow notifications only over HTTPS or on localhost.'
    : 'Allow notifications for Pipedash in your browser or system settings, then turn this on again.'
}

const permissionGranted = (): Promise<boolean> =>
  isTauri() ? isPermissionGranted() : Promise.resolve(Notification.permission === 'granted')

export async function requestSystemNotificationPermission(): Promise<boolean> {
  if (!systemNotificationsSupported()) {
    return false
  }

  try {
    if (await permissionGranted()) {
      return true
    }

    const result = isTauri() ? await requestPermission() : await Notification.requestPermission()

    return result === 'granted'
  } catch (error) {
    logger.warn('systemNotifications', "Couldn't request notification permission", error)

    return false
  }
}

async function deliver(title: string, body: string, notificationKey: string): Promise<void> {
  if (!systemNotificationsSupported() || !(await permissionGranted())) {
    return
  }

  if (isTauri()) {
    await tauriService.showSystemNotification(title, body, notificationKey)

    return
  }

  const notification = new Notification(title, { body })

  notification.onclick = () => {
    window.focus()
    openNotificationDetails(notificationKey)
  }
}

export function showSystemNotification(title: string, body: string, notificationKey: string): void {
  deliver(title, body, notificationKey).catch((error: unknown) =>
    logger.warn('systemNotifications', "Couldn't show notification", error)
  )
}
