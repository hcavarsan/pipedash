import { invoke } from '@tauri-apps/api/core'
import { requestPermission } from '@tauri-apps/plugin-notification'

import { logger } from '../lib/logger'
import { isTauri, tauriService } from '../services'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import { useNotificationStore } from '../stores/notificationStore'
import type { NotificationTarget } from '../types'

import { isAndroidPlatform, isMobilePlatform } from './platform'

export const openNotificationDetails = (key: string): void => {
  useNotificationCenterStore.getState().open({ type: 'notification', key })
}

export const systemDeliveryEnabled = (): boolean =>
  !isMobilePlatform() && useNotificationStore.getState().notificationsEnabled

export const systemNotificationsSupported = (): boolean =>
  isTauri() || ('Notification' in window && window.isSecureContext)

export function notificationProblem(kind: 'unsupported' | 'blocked'): string {
  if (isMobilePlatform()) {
    if (kind === 'unsupported') {
      return "Notifications aren't available on this device."
    }

    return isAndroidPlatform()
      ? 'Notifications are off for Pipedash. Turn them on in Android Settings > Apps > Pipedash > Notifications, then try again.'
      : 'Notifications are off for Pipedash. Turn them on in the Settings app under Notifications > Pipedash, then try again.'
  }

  return kind === 'unsupported'
    ? 'Browsers allow notifications only over HTTPS or on localhost.'
    : 'Allow notifications for Pipedash in your browser or system settings, then turn this on again.'
}

const permissionGranted = async (): Promise<boolean> =>
  isTauri()
    ? (await invoke<boolean | null>('plugin:notification|is_permission_granted')) === true
    : Notification.permission === 'granted'

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

async function deliver(title: string, body: string, target: NotificationTarget): Promise<void> {
  if (!systemNotificationsSupported() || !(await permissionGranted())) {
    return
  }

  if (isTauri()) {
    await tauriService.showSystemNotification(title, body, target)

    return
  }

  const notification = new Notification(title, { body })

  notification.onclick = () => {
    window.focus()
    openNotificationDetails(target.key)
  }
}

export function showSystemNotification(title: string, body: string, target: NotificationTarget): void {
  deliver(title, body, target).catch((error: unknown) =>
    logger.warn('systemNotifications', "Couldn't show notification", error)
  )
}
