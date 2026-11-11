import type { NotificationPreferenceSnapshot } from '../../types/notification_preferences'
import type { NotificationCenterSnapshot } from '../../types/notifications'
import type { HttpClient } from '../http'
import type { NotificationsService } from '../types'

export function createNotificationsApi(client: HttpClient): NotificationsService {
  return {
    updateNotificationCenter: async (snapshot, change) =>
      client.post<NotificationCenterSnapshot>('/notifications/center', { snapshot, change }),

    changeNotificationPreference: async (change, current) =>
      client.request<NotificationPreferenceSnapshot>('PATCH', '/devices/current/notification-preferences', {
        ...change,
        preferences: current,
      }),
  }
}
