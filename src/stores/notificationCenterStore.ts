import { create } from 'zustand'
import { logger } from '../lib/logger'
import { service } from '../services/service'
import type { CenterNotification, NotificationCenterChange } from '../types/notifications'

export type NotificationInput = Omit<CenterNotification, 'read'>

export type CenterSelection = { type: 'provider'; providerId: number } | { type: 'notification'; key: string }

interface NotificationCenterState {
  items: CenterNotification[]
  revision: number
  unread: number
  selection: CenterSelection | null
  push: (input: NotificationInput) => Promise<void>
  insertMissed: (input: NotificationInput) => Promise<void>
  markAllRead: () => Promise<void>
  markRead: (key: string) => Promise<void>
  clear: () => Promise<void>
  open: (selection: CenterSelection) => void
  close: () => void
}

let pending: Promise<void> = Promise.resolve()

export const useNotificationCenterStore = create<NotificationCenterState>()((set, get) => {
  const update = (change: NotificationCenterChange): Promise<void> => {
    pending = pending
      .then(async () => {
        const { items, revision, unread } = get()
        const next = await service.updateNotificationCenter({ items, revision, unread }, change)

        set({ items: next.items, revision: next.revision, unread: next.unread })
      })
      .catch((error: unknown) =>
        logger.warn('notificationCenterStore', "Couldn't update the notification center", error)
      )

    return pending
  }

  return {
    items: [],
    revision: 0,
    unread: 0,
    selection: null,

    push: (input) => update({ type: 'push', notice: { ...input, read: false } }),

    insertMissed: (input) => update({ type: 'insert_missed', notice: { ...input, read: false } }),

    markAllRead: () => update({ type: 'read', key: null }),

    markRead: (key) => update({ type: 'read', key }),

    clear: () => update({ type: 'clear' }),

    open: (selection) => {
      set({ selection })
      if (selection.type === 'notification') {
        void get().markRead(selection.key)
      }
    },

    close: () => set({ selection: null }),
  }
})
