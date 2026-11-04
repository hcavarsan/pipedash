import { create } from 'zustand'

import type { AlertLink, HealthLevel } from '../types/domain'

export const MAX_CENTER_NOTIFICATIONS = 50

export type CenterNotificationTone = 'danger' | 'warning' | 'success'

export interface CenterNotification {
  key: string
  at: number
  read: boolean
  tone: CenterNotificationTone
  level: HealthLevel | null
  title: string
  body: string
  providerIds: number[]
  pipelineId: string | null
  link: AlertLink | null
  runNumber?: number | null
}

export type NotificationInput = Omit<CenterNotification, 'read'>

export type CenterSelection = { type: 'provider'; providerId: number } | { type: 'notification'; key: string }

interface NotificationCenterState {
  items: CenterNotification[]
  selection: CenterSelection | null
  push: (input: NotificationInput) => void
  markAllRead: () => void
  markRead: (key: string) => void
  clear: () => void
  open: (selection: CenterSelection) => void
  close: () => void
}

export const useNotificationCenterStore = create<NotificationCenterState>()((set, get) => ({
  items: [],
  selection: null,

  push: (input) =>
    set((state) => ({
      items: [{ ...input, read: false }, ...state.items.filter((existing) => existing.key !== input.key)].slice(
        0,
        MAX_CENTER_NOTIFICATIONS
      ),
    })),

  markAllRead: () => {
    set((state) =>
      state.items.every((item) => item.read) ? state : { items: state.items.map((item) => ({ ...item, read: true })) }
    )
  },

  markRead: (key) => {
    set((state) => ({ items: state.items.map((item) => (item.key === key ? { ...item, read: true } : item)) }))
  },

  clear: () => {
    set({ items: [] })
  },

  open: (selection) => {
    set({ selection })
    if (selection.type === 'notification') {
      get().markRead(selection.key)
    }
  },

  close: () => set({ selection: null }),
}))
