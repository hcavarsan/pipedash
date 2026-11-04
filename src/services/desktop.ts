import { invoke } from '@tauri-apps/api/core'

import type { CenterNotification } from '../stores/notificationCenterStore'
import type { DesktopSettingsPatch, DesktopStatus } from '../types/domain'
import { toPipedashError } from '../types/errors'

export const DESKTOP_STATUS_EVENT = 'desktop-status-changed'
export const DESKTOP_NOTIFICATIONS_EVENT = 'desktop-notifications-changed'

export interface DesktopNotificationSnapshot {
  revision: number
  items: CenterNotification[]
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw toPipedashError(error)
  }
}

export const desktopService = {
  getStatus: (): Promise<DesktopStatus> => call('get_desktop_status'),
  updateSettings: (patch: DesktopSettingsPatch): Promise<DesktopStatus> => call('update_desktop_settings', { patch }),
  importNotificationPreference: (enabled: boolean): Promise<DesktopStatus> =>
    call('import_desktop_notification_preference', { enabled }),
  testNotification: (): Promise<void> => call('test_desktop_notification'),
  getNotifications: (): Promise<DesktopNotificationSnapshot> => call('get_desktop_notifications'),
  markNotificationsRead: (key: string | null): Promise<DesktopNotificationSnapshot> =>
    call('mark_desktop_notifications_read', { key }),
  clearNotifications: (): Promise<DesktopNotificationSnapshot> => call('clear_desktop_notifications'),
}
