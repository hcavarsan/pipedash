import { create } from 'zustand'
import { logger } from '../lib/logger'
import { service } from '../services/service'
import type { CenterNotification, NotificationCenterChange } from '../types/notifications'
import { displayErrorNotification } from '../utils/errorDisplay'

export type NotificationInput = Omit<CenterNotification, 'read'>

export type CenterSelection = { type: 'provider'; providerId: number } | { type: 'notification'; key: string }

interface NotificationCenterState {
  items: CenterNotification[]
  revision: number
  unread: number
  selection: CenterSelection | null
  pending: 'markAll' | 'clear' | null
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
    const run = pending.then(async () => {
      const { items, revision, unread } = get()
      const next = await service.updateNotificationCenter({ items, revision, unread }, change)

      set({ items: next.items, revision: next.revision, unread: next.unread })
    })
    pending = run.catch(() => undefined)

    return run
  }
  const reportFailure = (change: NotificationCenterChange, title: string): Promise<void> =>
    update(change).catch((error: unknown) => displayErrorNotification(error, title))
  const runBulk = (kind: 'markAll' | 'clear', change: NotificationCenterChange, title: string): Promise<void> => {
    if (get().pending !== null) {
      return Promise.resolve()
    }

    set({ pending: kind })

    return reportFailure(change, title).finally(() => set({ pending: null }))
  }

  return {
    items: [],
    revision: 0,
    unread: 0,
    selection: null,
    pending: null,

    push: (input) =>
      update({ type: 'push', notice: { ...input, read: false } }).catch((error: unknown) =>
        logger.warn('notificationCenterStore', "Couldn't update the notification center", error)
      ),

    insertMissed: (input) =>
      update({ type: 'insert_missed', notice: { ...input, read: false } }).catch((error: unknown) =>
        logger.warn('notificationCenterStore', "Couldn't update the notification center", error)
      ),

    markAllRead: () => runBulk('markAll', { type: 'read', key: null }, "Couldn't mark notifications as read"),

    markRead: (key) => reportFailure({ type: 'read', key }, "Couldn't mark the notification as read"),

    clear: () => runBulk('clear', { type: 'clear' }, "Couldn't clear notifications"),

    open: (selection) => {
      set({ selection })
      if (selection.type === 'notification') {
        void get().markRead(selection.key)
      }
    },

    close: () => set({ selection: null }),
  }
})
