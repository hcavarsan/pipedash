import { invoke } from '@tauri-apps/api/core'

import type { NotificationPreferenceSnapshot } from '../../types/notification_preferences'
import type { NotificationCenterSnapshot } from '../../types/notifications'
import type { NotificationsService } from '../types'

export const notificationsTauriService: NotificationsService = {
  updateNotificationCenter: async (snapshot, change) =>
    invoke<NotificationCenterSnapshot>('update_notification_center', { snapshot, change }),

  changeNotificationPreference: async (change) =>
    invoke<NotificationPreferenceSnapshot>('change_notification_preference', { change }),
}
