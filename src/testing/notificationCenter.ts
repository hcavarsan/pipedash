import { mock } from 'node:test'

import { apiService } from '../services/api'
import { tauriService } from '../services/tauri'
import type { CenterNotification, NotificationCenterChange, NotificationCenterSnapshot } from '../types/notifications'

const LIMIT = 50

function items(snapshot: NotificationCenterSnapshot, change: NotificationCenterChange): CenterNotification[] {
  switch (change.type) {
    case 'push':
      return [change.notice, ...snapshot.items.filter((item) => item.key !== change.notice.key)].slice(0, LIMIT)
    case 'insert_missed':
      return [...snapshot.items.filter((item) => item.key !== change.notice.key), change.notice]
        .toSorted((a, b) => b.at - a.at)
        .slice(0, LIMIT)
    case 'read':
      return snapshot.items.map((item) =>
        change.key === null || item.key === change.key ? { ...item, read: true } : item
      )
    case 'clear':
      return []
  }
}

export function reduceNotificationCenter(
  snapshot: NotificationCenterSnapshot,
  change: NotificationCenterChange
): NotificationCenterSnapshot {
  const next = items(snapshot, change)

  return { revision: snapshot.revision + 1, items: next, unread: next.filter((item) => !item.read).length }
}

export function installNotificationCenterDouble(): void {
  const double = async (snapshot: NotificationCenterSnapshot, change: NotificationCenterChange) =>
    reduceNotificationCenter(snapshot, change)

  mock.method(apiService, 'updateNotificationCenter', double)
  mock.method(tauriService, 'updateNotificationCenter', double)
}
